use crate::{model::*, protocol::*};
use anyhow::{anyhow, bail, ensure, Result};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

thread_local! {
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}
pub fn set_cancellation(flag: Arc<AtomicBool>) {
    CANCEL.with(|slot| *slot.borrow_mut() = Some(flag));
}
pub(crate) fn check_cancelled() -> Result<()> {
    let cancelled = CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    });
    ensure!(!cancelled, "Bluetooth operation cancelled");
    Ok(())
}
use windows::{
    core::{RuntimeType, GUID},
    Devices::{
        Bluetooth::{
            BluetoothCacheMode, BluetoothDevice, BluetoothLEDevice, GenericAttributeProfile::*,
        },
        Enumeration::DeviceInformation,
    },
    Foundation::{AsyncStatus, EventRegistrationToken, IAsyncOperation, TypedEventHandler},
    Storage::Streams::{DataReader, DataWriter},
};

pub(crate) fn wait<T: RuntimeType>(op: IAsyncOperation<T>, seconds: u64) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Err(error) = check_cancelled() {
            let _ = op.Cancel();
            return Err(error);
        }
        let status = op.Status()?;
        if status == AsyncStatus::Completed {
            return Ok(op.GetResults()?);
        }
        if status == AsyncStatus::Error || status == AsyncStatus::Canceled {
            return Ok(op.GetResults()?);
        }
        if Instant::now() >= deadline {
            let _ = op.Cancel();
            bail!("Bluetooth request timed out");
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}
struct GattLink {
    device: BluetoothLEDevice,
    service: GattDeviceService,
    session: GattSession,
    characteristic: GattCharacteristic,
    token: EventRegistrationToken,
    rx: mpsc::Receiver<Result<Frame>>,
}

enum Link {
    Gatt(GattLink),
    Rfcomm(crate::rfcomm::RfcommTransport),
}

impl Link {
    fn discard_ready(&mut self) -> Result<()> {
        match self {
            Self::Gatt(gatt) => {
                while gatt.rx.try_recv().is_ok() {}
                Ok(())
            }
            Self::Rfcomm(rfcomm) => rfcomm.discard_ready(),
        }
    }

    fn send(&mut self, packet: &[u8]) -> Result<()> {
        match self {
            Self::Gatt(gatt) => {
                for segment in encode(packet, gatt.session.MaxPduSize()? as usize)? {
                    let writer = DataWriter::new()?;
                    writer.WriteBytes(&segment)?;
                    let result = wait(
                        gatt.characteristic.WriteValueWithResultAndOptionAsync(
                            &writer.DetachBuffer()?,
                            GattWriteOption::WriteWithResponse,
                        )?,
                        4,
                    )?;
                    ensure!(
                        result.Status()? == GattCommunicationStatus::Success,
                        "Bluetooth write failed: {:?}",
                        result.Status()?
                    );
                }
                Ok(())
            }
            Self::Rfcomm(rfcomm) => rfcomm.send(packet),
        }
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<Frame>> {
        match self {
            Self::Gatt(gatt) => match gatt.rx.recv_timeout(timeout) {
                Ok(Ok(frame)) => Ok(Some(frame)),
                Ok(Err(error)) => Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
                Err(_) => bail!("Headphone connection closed"),
            },
            Self::Rfcomm(rfcomm) => rfcomm.recv_frame(timeout),
        }
    }
}

impl Drop for GattLink {
    fn drop(&mut self) {
        let _ = self.characteristic.RemoveValueChanged(self.token);
        if let Ok(op) = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::None,
            )
        {
            let _ = wait(op, 2);
        }
        let _ = self.session.SetMaintainConnection(false);
        let _ = self.session.Close();
        let _ = self.service.Close();
        let _ = self.device.Close();
    }
}

pub struct Connection {
    link: Link,
    output: Option<crate::audio::OutputMonitor>,
    pub state: State,
}
impl Connection {
    pub fn connect(address: Option<u64>) -> Result<(Self, u64)> {
        let address = Self::find_address(address)?;
        let mut connection = match Self::connect_gatt(address) {
            Ok(connection) => connection,
            Err(gatt_error) => {
                check_cancelled()?;
                Self::connect_rfcomm(address).map_err(|rfcomm_error| {
                    anyhow!(
                        "BLE/GATT failed ({gatt_error:#}); RFCOMM fallback failed ({rfcomm_error:#})"
                    )
                })?
            }
        };
        connection.output = match crate::audio::OutputMonitor::new(address) {
            Ok(output) => Some(output),
            Err(error) => {
                log::warn!("Windows audio output monitor unavailable: {error:#}");
                None
            }
        };
        connection.update_output();
        Ok((connection, address))
    }

    fn find_address(address: Option<u64>) -> Result<u64> {
        if let Some(address) = address {
            return Ok(address);
        }
        let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
        let devices = wait(DeviceInformation::FindAllAsyncAqsFilter(&selector)?, 10)?;
        for info in devices {
            if info.Name()?.to_string().to_lowercase().starts_with("bose") {
                let device = wait(BluetoothLEDevice::FromIdAsync(&info.Id()?)?, 10)?;
                let address = device.BluetoothAddress()?;
                let _ = device.Close();
                return Ok(address);
            }
        }

        let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
        let devices = wait(DeviceInformation::FindAllAsyncAqsFilter(&selector)?, 10)?;
        for info in devices {
            if info.Name()?.to_string().to_lowercase().starts_with("bose") {
                let device = wait(BluetoothDevice::FromIdAsync(&info.Id()?)?, 10)?;
                let address = device.BluetoothAddress()?;
                let _ = device.Close();
                return Ok(address);
            }
        }
        Err(anyhow!("No paired Bose headphones found"))
    }

    fn connect_gatt(address: u64) -> Result<Self> {
        let mut last_error = None;
        for (attempt, cache_mode) in [BluetoothCacheMode::Cached, BluetoothCacheMode::Uncached]
            .into_iter()
            .enumerate()
        {
            if attempt > 0 {
                check_cancelled()?;
                std::thread::sleep(Duration::from_millis(150));
            }
            match Self::connect_gatt_with_cache(address, cache_mode) {
                Ok(connection) => return Ok(connection),
                Err(error) => {
                    if attempt == 0 {
                        log::warn!(
                            "BLE/GATT cached connection failed ({error:#}); retrying with uncached discovery"
                        );
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("BLE/GATT connection failed")))
    }

    fn connect_gatt_with_cache(address: u64, cache_mode: BluetoothCacheMode) -> Result<Self> {
        let device = wait(BluetoothLEDevice::FromBluetoothAddressAsync(address)?, 10)?;
        let services = match wait(
            device.GetGattServicesForUuidWithCacheModeAsync(
                GUID::from_u128(0x0000febe_0000_1000_8000_00805f9b34fb),
                cache_mode,
            )?,
            10,
        ) {
            Ok(services) => services,
            Err(error) => {
                let _ = device.Close();
                return Err(error);
            }
        };
        if services.Status()? != GattCommunicationStatus::Success {
            let status = services.Status()?;
            let _ = device.Close();
            bail!("Bluetooth services unavailable ({status:?}, cache mode {cache_mode:?})");
        }
        let mut chosen = None;
        for service in services.Services()? {
            let result = wait(service.GetCharacteristicsWithCacheModeAsync(cache_mode)?, 8)?;
            if result.Status()? == GattCommunicationStatus::Success {
                let mut fallback = None;
                for c in result.Characteristics()? {
                    let uuid = c.Uuid()?;
                    if uuid == GUID::from_u128(0xc65b8f2f_aee2_4c89_b758_bc4892d6f2d8) {
                        chosen = Some((service.clone(), c));
                        break;
                    }
                    if uuid == GUID::from_u128(0xd417c028_9818_4354_99d1_2ac09d074591) {
                        fallback = Some(c);
                    }
                }
                if chosen.is_none() {
                    if let Some(c) = fallback {
                        chosen = Some((service.clone(), c));
                    }
                }
            }
            if chosen.is_some() {
                break;
            }
            let _ = service.Close();
        }
        let (service, characteristic) =
            chosen.ok_or_else(|| anyhow!("Bose management interface unavailable"))?;
        let session = wait(
            GattSession::FromDeviceIdAsync(&device.BluetoothDeviceId()?)?,
            8,
        )?;
        session.SetMaintainConnection(true)?;
        let (tx, rx) = mpsc::sync_channel(256);
        let decoders = Arc::new(Mutex::new((Framing::default(), Decoder::default())));
        let token = characteristic.ValueChanged(&TypedEventHandler::new(
            move |_: &Option<GattCharacteristic>, args: &Option<GattValueChangedEventArgs>| {
                let result = (|| -> Result<Vec<Frame>> {
                    let args = args.as_ref().ok_or_else(|| anyhow!("Empty GATT event"))?;
                    let reader = DataReader::FromBuffer(&args.CharacteristicValue()?)?;
                    let mut bytes = vec![0; reader.UnconsumedBufferLength()? as usize];
                    reader.ReadBytes(&mut bytes)?;
                    let mut pair = decoders.lock().map_err(|_| anyhow!("GATT decoder lock"))?;
                    match pair.0.push(&bytes)? {
                        Some(raw) => pair.1.push(&raw),
                        None => Ok(vec![]),
                    }
                })();
                match result {
                    Ok(frames) => {
                        for f in frames {
                            let _ = tx.try_send(Ok(f));
                        }
                    }
                    Err(e) => {
                        let _ = tx.try_send(Err(e));
                    }
                }
                Ok(())
            },
        ))?;
        let status = wait(
            characteristic.WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )?,
            8,
        )?;
        if status != GattCommunicationStatus::Success {
            let _ = characteristic.RemoveValueChanged(token);
            let _ = session.SetMaintainConnection(false);
            let _ = session.Close();
            let _ = service.Close();
            let _ = device.Close();
            bail!(
                "Could not subscribe to headphone notifications ({status:?}, cache mode {cache_mode:?})"
            );
        }
        let name = device.Name()?.to_string();
        let mut conn = Self {
            output: None,
            link: Link::Gatt(GattLink {
                device,
                service,
                session,
                characteristic,
                token,
                rx,
            }),
            state: State {
                name,
                transport: "Bluetooth LE".into(),
                ..State::default()
            },
        };
        conn.initialize()?;
        conn.read_initial()?;
        conn.state.connected = true;
        Ok(conn)
    }

    fn connect_rfcomm(address: u64) -> Result<Self> {
        let mut conn = Self {
            output: None,
            link: Link::Rfcomm(crate::rfcomm::RfcommTransport::connect(address, 2)?),
            state: State {
                name: "Bose headphones".into(),
                transport: "RFCOMM".into(),
                ..State::default()
            },
        };
        conn.initialize()?;
        conn.read_initial()?;
        conn.state.connected = true;
        Ok(conn)
    }
    fn request(
        &mut self,
        block: u8,
        function: u8,
        op: u8,
        payload: &[u8],
        drain: bool,
    ) -> Result<Vec<Frame>> {
        self.request_with_timeout(block, function, op, payload, drain, Duration::from_secs(4))
    }
    fn request_with_timeout(
        &mut self,
        block: u8,
        function: u8,
        op: u8,
        payload: &[u8],
        drain: bool,
        timeout: Duration,
    ) -> Result<Vec<Frame>> {
        check_cancelled()?;
        self.link.discard_ready()?;
        let p = packet(block, function, op, payload)?;
        self.link.send(&p)?;
        let mut result = vec![];
        let mut deadline = Instant::now() + timeout;
        loop {
            check_cancelled()?;
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                if drain && !result.is_empty() {
                    return Ok(result);
                }
                bail!("Headphones did not complete [{block}.{function}]");
            }
            match self.link.recv(remain.min(Duration::from_millis(50)))? {
                Some(f) => {
                    let matching =
                        f.block == block && (f.function == function || (drain && f.function == 6));
                    if !matching || f.op == 7 {
                        continue;
                    }
                    if f.op == 4 {
                        return Err(DeviceRejected {
                            block,
                            function,
                            code: f.payload.first().copied().unwrap_or(0),
                        }
                        .into());
                    }
                    if !matches!(f.op, 3 | 6)
                        || (block == 4 && matches!(function, 1 | 2) && op == 5 && f.op != 6)
                    {
                        continue;
                    }
                    result.push(f);
                    if !drain {
                        return Ok(result);
                    }
                    deadline = Instant::now() + Duration::from_millis(350);
                }
                None => continue,
            }
        }
    }
    fn get(&mut self, b: u8, f: u8, min: usize) -> Result<Vec<u8>> {
        status(self.request(b, f, 1, &[], false)?.remove(0), min)
    }
    fn set(&mut self, b: u8, f: u8, p: &[u8]) -> Result<()> {
        let r = self.request(b, f, 2, p, false)?.remove(0);
        status(r, 1)?;
        Ok(())
    }
    fn start(&mut self, b: u8, f: u8, p: &[u8]) -> Result<()> {
        let r = self.request(b, f, 5, p, false)?.remove(0);
        status(r, 0)?;
        Ok(())
    }
    fn start_with_timeout(&mut self, b: u8, f: u8, p: &[u8], timeout: Duration) -> Result<()> {
        let r = self
            .request_with_timeout(b, f, 5, p, false, timeout)?
            .remove(0);
        status(r, 0)?;
        Ok(())
    }
    fn confirm(&mut self, mut read: impl FnMut(&mut Self) -> Result<bool>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if read(self)? {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "Headphones did not confirm the setting"
            );
            check_cancelled()?;
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    fn initialize(&mut self) -> Result<()> {
        self.state.protocol = String::from_utf8_lossy(&self.get(0, 1, 1)?)
            .trim_end_matches(char::from(0))
            .into();
        let blocks = self.get(0, 2, 1)?;
        let product = self.get(0, 3, 3)?;
        ensure!(product.len() >= 3, "Incomplete product identity");
        self.state.product_id = Some(u16::from_be_bytes([product[0], product[1]]));
        if let Ok(guid) = self.get(0, 12, 16) {
            if guid.len() == 16 {
                self.state.product_guid = guid
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        format!(
                            "{}{b:02x}",
                            if [4, 6, 8, 10].contains(&i) { "-" } else { "" }
                        )
                    })
                    .collect();
            }
        }
        let supported: BTreeSet<usize> = blocks
            .iter()
            .rev()
            .enumerate()
            .flat_map(|(i, b)| {
                (0..8).filter_map(move |bit| {
                    if b & (1 << bit) != 0 {
                        Some(i * 8 + bit)
                    } else {
                        None
                    }
                })
            })
            .collect();
        if supported.contains(&9) {
            let mut mask = 0u32;
            for b in [0, 1, 2, 4, 5, 9, 16, 18, 31] {
                if supported.contains(&b) {
                    mask |= 1 << b;
                }
            }
            let mut p = vec![1];
            p.extend(mask.to_be_bytes());
            self.set(9, 2, &p)?;
        }
        Ok(())
    }
    fn audio(&mut self) -> Result<()> {
        let p = self.get(31, 10, 5)?;
        self.state.cnc = p[0] as i32;
        self.state.spatial = p[2];
        self.state.wind = p[3] != 0;
        self.state.anc = p[4] != 0;
        Ok(())
    }
    fn eq(&mut self) -> Result<()> {
        self.state.eq = parse_eq(&self.get(1, 7, 4)?)?;
        Ok(())
    }
    fn read_multipoint(&mut self) -> Result<()> {
        let flags = self.get(1, 10, 1)?[0];
        self.state.multipoint = (flags & 2 != 0).then_some(flags & 1 != 0);
        Ok(())
    }
    fn write_audio(&mut self) -> Result<()> {
        let expected = (
            self.state.cnc,
            self.state.spatial,
            self.state.wind,
            self.state.anc,
        );
        self.set(
            31,
            10,
            &[
                self.state.cnc as u8,
                0,
                self.state.spatial,
                self.state.wind as u8,
                self.state.anc as u8,
            ],
        )?;
        self.confirm(|conn| {
            conn.audio()?;
            Ok((
                conn.state.cnc,
                conn.state.spatial,
                conn.state.wind,
                conn.state.anc,
            ) == expected)
        })
    }
    fn read_initial(&mut self) -> Result<()> {
        let p = self.get(1, 2, 2)?;
        self.state.name = String::from_utf8_lossy(&p[1..])
            .trim_end_matches(char::from(0))
            .into();
        self.state.firmware = String::from_utf8_lossy(&self.get(0, 5, 1)?)
            .trim_end_matches(char::from(0))
            .into();
        self.eq()?;
        self.state.sidetone = self.get(1, 11, 2).ok().map(|p| p[1]);
        self.state.pause = self.get(1, 24, 1).ok().map(|p| p[0] != 0);
        self.state.answer = self.get(1, 27, 1).ok().map(|p| p[0] != 0);
        if let Ok(p) = self.get(31, 8, 3) {
            self.state.total_modes = p[0];
            self.state.favorites = p[2];
        }
        if let Ok(frames) = self.request(31, 1, 5, &[], true) {
            for f in frames {
                if f.function == 6 && f.payload.len() >= 38 {
                    let p = f.payload;
                    let name = String::from_utf8_lossy(&p[6..38])
                        .split(char::from(0))
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_owned();
                    if !name.is_empty() && name != "None" && (p.len() < 48 || p[4] != 0 || p[0] < 4)
                    {
                        self.state.modes.insert(p[0], name);
                    }
                }
            }
        }
        self.poll_with_devices(true)
    }
    pub fn poll(&mut self) -> Result<()> {
        self.poll_with_devices(false)
    }
    pub fn poll_with_devices(&mut self, refresh_devices: bool) -> Result<()> {
        let p = self.get(2, 2, 1)?;
        ensure!(p[0] <= 100, "Invalid battery value");
        self.state.battery = Some(p[0]);
        self.state.mode = self.get(31, 3, 1)?[0];
        self.audio()?;
        self.state.charging = self.get(2, 3, 1).ok().map(|p| p[0] != 0);
        let _ = self.read_multipoint();
        if let Ok(p) = self.get(1, 37, 1) {
            self.state.le_audio = Some(p[0] & 1 != 0);
        }
        self.eq()?;
        if self.state.pause.is_some() {
            self.state.pause = Some(self.get(1, 24, 1)?[0] != 0);
        }
        if self.state.answer.is_some() {
            self.state.answer = Some(self.get(1, 27, 1)?[0] != 0);
        }
        if self.state.sidetone.is_some() {
            self.state.sidetone = Some(self.get(1, 11, 2)?[1]);
        }
        if refresh_devices {
            self.refresh_devices()
        } else {
            self.source()
        }
    }
    pub fn source(&mut self) -> Result<()> {
        let p = self.get(5, 1, 3)?;
        self.state.active = if matches!(p[2], 1 | 4) && p.len() >= 9 {
            Some(mac(&p[3..9]))
        } else {
            None
        };
        self.state.playback = match self.get(5, 4, 1) {
            Ok(p) => match p[0] {
                0 => Some(Playback::Stopped),
                1 => Some(Playback::Playing),
                2 => Some(Playback::Paused),
                3 | 4 => Some(Playback::Seeking),
                _ => None,
            },
            Err(error) if error.downcast_ref::<DeviceRejected>().is_some() => None,
            Err(error) => return Err(error),
        };
        self.update_output();
        Ok(())
    }
    fn update_output(&mut self) {
        if self
            .output
            .as_mut()
            .is_some_and(|output| output.is_playing(self.state.active.as_deref()))
        {
            self.state.playback = Some(Playback::Playing);
        }
    }
    fn refresh_devices(&mut self) -> Result<()> {
        let p = self.get(4, 4, 1)?;
        let mut devices = vec![];
        for bytes in p[1..].as_chunks::<6>().0 {
            if bytes.iter().all(|b| *b == 0) {
                continue;
            }
            let address = mac(bytes);
            let reply = self.request(4, 5, 1, bytes, false)?.remove(0);
            let p = status(reply, 9)?;
            let name = String::from_utf8_lossy(&p[9..])
                .trim_matches(|c: char| c.is_control())
                .to_owned();
            devices.push(Paired {
                mac: address.clone(),
                name: if name.is_empty() { address } else { name },
                connected: p[6] & 1 != 0,
            });
        }
        self.state.devices = devices;
        self.source()
    }
    pub fn execute(&mut self, command: &Command) -> Result<Option<i32>> {
        match command {
            Command::Slider(edit) => match edit.key {
                SliderKey::Cnc => {
                    ensure!((0..=10).contains(&edit.value), "Invalid noise level");
                    self.audio()?;
                    self.state.cnc = edit.value;
                    self.write_audio()?;
                    return Ok(Some(self.state.cnc));
                }
                SliderKey::Eq(id) => {
                    let b = self
                        .state
                        .eq
                        .iter()
                        .find(|b| b.id == id)
                        .ok_or_else(|| anyhow!("Unknown EQ band"))?;
                    ensure!((b.min..=b.max).contains(&edit.value), "Invalid EQ value");
                    self.set(1, 7, &[edit.value as i8 as u8, id])?;
                    self.confirm(|conn| {
                        conn.eq()?;
                        Ok(conn
                            .state
                            .eq
                            .iter()
                            .any(|b| b.id == id && b.value == edit.value))
                    })?;
                    return Ok(Some(edit.value));
                }
            },
            Command::ResetEq(_) => {
                for b in self.state.eq.clone() {
                    self.set(1, 7, &[0i32.clamp(b.min, b.max) as i8 as u8, b.id])?;
                }
                self.confirm(|conn| {
                    conn.eq()?;
                    Ok(conn
                        .state
                        .eq
                        .iter()
                        .all(|b| b.value == 0i32.clamp(b.min, b.max)))
                })?;
            }
            Command::Mode(mode) => {
                self.start(31, 3, &[*mode, 0])?;
                self.confirm(|conn| {
                    conn.state.mode = conn.get(31, 3, 1)?[0];
                    Ok(conn.state.mode == *mode)
                })?;
                self.audio()?;
            }
            Command::Spatial(v) => {
                self.audio()?;
                self.state.spatial = *v;
                self.write_audio()?;
            }
            Command::Wind(v) => {
                self.audio()?;
                self.state.wind = *v;
                self.write_audio()?;
            }
            Command::Connectivity {
                setting, enabled, ..
            } => match setting {
                ConnectivitySetting::Multipoint => {
                    self.set(1, 10, &[*enabled as u8])?;
                    self.confirm(|conn| {
                        conn.read_multipoint()?;
                        Ok(conn.state.multipoint == Some(*enabled))
                    })?;
                    self.refresh_devices()?;
                }
                ConnectivitySetting::LeAudio => {
                    self.set(1, 37, &[*enabled as u8])?;
                    self.confirm(|conn| {
                        conn.state.le_audio = Some(conn.get(1, 37, 1)?[0] & 1 != 0);
                        Ok(conn.state.le_audio == Some(*enabled))
                    })?;
                    let _ = self.read_multipoint();
                }
            },
            Command::Pause(v) => {
                self.set(1, 24, &[*v as u8])?;
                self.confirm(|conn| {
                    conn.state.pause = Some(conn.get(1, 24, 1)?[0] != 0);
                    Ok(conn.state.pause == Some(*v))
                })?;
            }
            Command::Answer(v) => {
                self.set(1, 27, &[*v as u8])?;
                self.confirm(|conn| {
                    conn.state.answer = Some(conn.get(1, 27, 1)?[0] != 0);
                    Ok(conn.state.answer == Some(*v))
                })?;
            }
            Command::Sidetone(v) => {
                ensure!(*v <= 3, "Invalid sidetone");
                self.set(1, 11, &[1, *v])?;
                self.confirm(|conn| {
                    conn.state.sidetone = Some(conn.get(1, 11, 2)?[1]);
                    Ok(conn.state.sidetone == Some(*v))
                })?;
            }
            Command::Favorite(id) => {
                ensure!(*id < 8, "Invalid favorite mode");
                let selected = self.state.favorites & (1 << id) != 0;
                let next = self.state.favorites ^ (1 << id);
                ensure!(
                    !selected || next.count_ones() >= MIN_FAVORITE_MODES,
                    "At least two favorite modes are required"
                );
                self.set(31, 8, &[self.state.total_modes, 0, next])?;
                self.state.favorites = self.get(31, 8, 3)?[2];
            }
            Command::ConnectDevice(address) => {
                let target = mac_bytes(address)?;
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut routed = false;
                for attempt in 0..5 {
                    if attempt > 0 {
                        check_cancelled()?;
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    let mut payload = Vec::with_capacity(7);
                    payload.push(0x82);
                    payload.extend_from_slice(&target);
                    match self.start_with_timeout(
                        4,
                        12,
                        &payload,
                        remaining.min(Duration::from_secs(4)),
                    ) {
                        Ok(()) => {
                            routed = true;
                            break;
                        }
                        Err(error)
                            if error
                                .downcast_ref::<DeviceRejected>()
                                .is_some_and(|rejection| rejection.code == 4) =>
                        {
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                }
                ensure!(
                    routed,
                    "The target device is not ready for audio switching; start playback on it and try again"
                );
                self.refresh_devices()?;
                ensure!(
                    self.state
                        .devices
                        .iter()
                        .any(|d| d.mac == *address && d.connected),
                    "The device did not confirm its connection"
                );
            }
            Command::DisconnectDevice(address) => {
                let target = mac_bytes(address)?;
                let response =
                    self.request_with_timeout(4, 2, 5, &target, false, Duration::from_secs(20))?;
                let reply = status(response.into_iter().next().unwrap(), 6)?;
                ensure!(reply[..6] == target[..], "Unexpected disconnected device");
                self.refresh_devices()?;
                ensure!(
                    !self
                        .state
                        .devices
                        .iter()
                        .any(|d| d.mac == *address && d.connected),
                    "The device did not confirm its disconnection"
                );
            }
            Command::Refresh => self.refresh_devices()?,
            Command::Rename(name) => {
                ensure!(
                    !name.trim().is_empty() && name.len() <= 32,
                    "Name must contain 1–32 UTF-8 bytes"
                );
                self.set(1, 2, name.as_bytes())?;
                let p = self.get(1, 2, 2)?;
                self.state.name = String::from_utf8_lossy(&p[1..])
                    .trim_end_matches(char::from(0))
                    .into();
            }
            Command::Pair => self.start(4, 8, &[1])?,
            Command::Power => self.start(7, 4, &[0])?,
            _ => {}
        }
        Ok(None)
    }
}
