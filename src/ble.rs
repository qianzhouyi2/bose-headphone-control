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
fn check_cancelled() -> Result<()> {
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
        Bluetooth::{BluetoothCacheMode, BluetoothLEDevice, GenericAttributeProfile::*},
        Enumeration::DeviceInformation,
    },
    Foundation::{AsyncStatus, EventRegistrationToken, IAsyncOperation, TypedEventHandler},
    Storage::Streams::{DataReader, DataWriter},
};

fn wait<T: RuntimeType>(op: IAsyncOperation<T>, seconds: u64) -> Result<T> {
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
pub struct Connection {
    device: BluetoothLEDevice,
    service: GattDeviceService,
    session: GattSession,
    characteristic: GattCharacteristic,
    token: EventRegistrationToken,
    rx: mpsc::Receiver<Result<Frame>>,
    pub state: State,
}
impl Connection {
    pub fn connect(address: Option<u64>) -> Result<(Self, u64)> {
        let device = if let Some(address) = address {
            wait(BluetoothLEDevice::FromBluetoothAddressAsync(address)?, 10)?
        } else {
            let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
            let devices = wait(DeviceInformation::FindAllAsyncAqsFilter(&selector)?, 10)?;
            let mut found = None;
            for info in devices {
                if info.Name()?.to_string().to_lowercase().starts_with("bose") {
                    found = Some(wait(BluetoothLEDevice::FromIdAsync(&info.Id()?)?, 10)?);
                    break;
                }
            }
            found.ok_or_else(|| anyhow!("No paired Bose headphones found"))?
        };
        let address = device.BluetoothAddress()?;
        let services = wait(
            device.GetGattServicesForUuidWithCacheModeAsync(
                GUID::from_u128(0x0000febe_0000_1000_8000_00805f9b34fb),
                BluetoothCacheMode::Uncached,
            )?,
            10,
        )?;
        ensure!(
            services.Status()? == GattCommunicationStatus::Success,
            "Bluetooth services unavailable"
        );
        let mut chosen = None;
        for service in services.Services()? {
            let result = wait(
                service.GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?,
                8,
            )?;
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
            bail!("Could not subscribe to headphone notifications");
        }
        let name = device.Name()?.to_string();
        let mut conn = Self {
            device,
            service,
            session,
            characteristic,
            token,
            rx,
            state: State {
                name,
                transport: "Bluetooth LE".into(),
                ..State::default()
            },
        };
        conn.initialize()?;
        conn.read_initial()?;
        conn.state.connected = true;
        Ok((conn, address))
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
        // Discard complete unsolicited frames only; decoders retain partial bytes.
        while self.rx.try_recv().is_ok() {}
        let p = packet(block, function, op, payload)?;
        for segment in encode(&p, self.session.MaxPduSize()? as usize)? {
            let writer = DataWriter::new()?;
            writer.WriteBytes(&segment)?;
            let r = wait(
                self.characteristic.WriteValueWithResultAndOptionAsync(
                    &writer.DetachBuffer()?,
                    GattWriteOption::WriteWithResponse,
                )?,
                4,
            )?;
            ensure!(
                r.Status()? == GattCommunicationStatus::Success,
                "Bluetooth write failed: {:?}",
                r.Status()?
            );
        }
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
            match self.rx.recv_timeout(remain.min(Duration::from_millis(50))) {
                Ok(Ok(f)) => {
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
                Ok(Err(e)) => return Err(e),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => bail!("Headphone connection timed out [{block}.{function}]"),
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
    fn initialize(&mut self) -> Result<()> {
        self.state.protocol = String::from_utf8_lossy(&self.get(0, 1, 1)?)
            .trim_end_matches(char::from(0))
            .into();
        let blocks = self.get(0, 2, 1)?;
        let product = self.get(0, 3, 3)?;
        ensure!(product.len() >= 3, "Incomplete product identity");
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
    fn write_audio(&mut self) -> Result<()> {
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
        self.audio()
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
        self.state.multipoint = self.get(1, 10, 1).ok().map(|p| p[0] & 2 != 0);
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
        self.poll()
    }
    pub fn poll(&mut self) -> Result<()> {
        let p = self.get(2, 2, 1)?;
        ensure!(p[0] <= 100, "Invalid battery value");
        self.state.battery = Some(p[0]);
        self.state.mode = self.get(31, 3, 1)?[0];
        self.audio()?;
        self.state.charging = self.get(2, 3, 1).ok().map(|p| p[0] != 0);
        self.eq()?;
        self.refresh_devices()?;
        Ok(())
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
        Ok(())
    }
    fn refresh_devices(&mut self) -> Result<()> {
        let p = self.get(4, 4, 1)?;
        let mut devices = vec![];
        for bytes in p[1..].chunks_exact(6) {
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
                    self.eq()?;
                    return Ok(Some(
                        self.state
                            .eq
                            .iter()
                            .find(|b| b.id == id)
                            .ok_or_else(|| anyhow!("Missing EQ confirmation"))?
                            .value,
                    ));
                }
            },
            Command::ResetEq(_) => {
                for b in self.state.eq.clone() {
                    self.set(1, 7, &[0i32.clamp(b.min, b.max) as i8 as u8, b.id])?;
                }
                self.eq()?;
            }
            Command::Mode(mode) => {
                self.start(31, 3, &[*mode, 0])?;
                self.state.mode = self.get(31, 3, 1)?[0];
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
            Command::Multipoint(v) => {
                self.set(1, 10, &[*v as u8])?;
                self.state.multipoint = Some(self.get(1, 10, 1)?[0] & 2 != 0);
                self.refresh_devices()?;
            }
            Command::Pause(v) => {
                self.set(1, 24, &[*v as u8])?;
                self.state.pause = Some(self.get(1, 24, 1)?[0] != 0);
            }
            Command::Answer(v) => {
                self.set(1, 27, &[*v as u8])?;
                self.state.answer = Some(self.get(1, 27, 1)?[0] != 0);
            }
            Command::Sidetone(v) => {
                ensure!(*v <= 3, "Invalid sidetone");
                self.set(1, 11, &[1, *v])?;
                self.state.sidetone = Some(self.get(1, 11, 2)?[1]);
            }
            Command::Favorite(id) => {
                ensure!(*id < 8, "Invalid favorite mode");
                self.set(
                    31,
                    8,
                    &[self.state.total_modes, 0, self.state.favorites ^ (1 << id)],
                )?;
                self.state.favorites = self.get(31, 8, 3)?[2];
            }
            Command::ConnectDevice(address) => {
                let target = mac_bytes(address)?;
                let mut p = vec![0];
                p.extend_from_slice(&target);
                let response =
                    self.request_with_timeout(4, 1, 5, &p, false, Duration::from_secs(20))?;
                let reply = status(response.into_iter().next().unwrap(), 7)?;
                ensure!(reply[..6] == target[..], "Unexpected connected device");
                ensure!(
                    reply[6] & 3 != 0 || reply.get(7).is_some_and(|b| b & 1 != 0),
                    "The device did not establish an audio connection"
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

impl Drop for Connection {
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
