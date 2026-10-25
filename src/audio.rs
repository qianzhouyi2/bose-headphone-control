use crate::{ble::wait, protocol::mac};
use anyhow::{ensure, Result};
use std::time::{Duration, Instant};
use windows::{
    core::{GUID, HSTRING, PCWSTR},
    Devices::Bluetooth::BluetoothAdapter,
    Win32::{
        Devices::{
            DeviceAndDriverInstallation::{
                CM_Get_DevNode_PropertyW, CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW,
                CM_Locate_DevNodeW, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL,
                CR_SUCCESS,
            },
            FunctionDiscovery::PKEY_Device_ContainerId,
            Properties::{DEVPKEY_Device_ContainerId, DEVPROPTYPE, DEVPROP_TYPE_GUID},
        },
        Media::Audio::{
            eRender, Endpoints::IAudioMeterInformation, IMMDeviceEnumerator, MMDeviceEnumerator,
            DEVICE_STATE_ACTIVE,
        },
        System::Com::{
            CoCreateInstance, StructuredStorage::PropVariantToGUID, CLSCTX_ALL, STGM_READ,
        },
    },
};

pub struct OutputMonitor {
    local_address: String,
    containers: Vec<GUID>,
    enumerator: IMMDeviceEnumerator,
    meters: Vec<IAudioMeterInformation>,
    next_refresh: Instant,
    last_signal: Option<Instant>,
}

impl OutputMonitor {
    pub fn new(headphone_address: u64) -> Result<Self> {
        let adapter = wait(BluetoothAdapter::GetDefaultAsync()?, 3)?;
        let address = adapter.BluetoothAddress()?.to_be_bytes();
        let local_address = mac(&address[2..]);
        let containers = headphone_containers(headphone_address)?;
        ensure!(
            !containers.is_empty(),
            "Headphone audio identity unavailable"
        );
        let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        Ok(Self {
            local_address,
            containers,
            enumerator,
            meters: Vec::new(),
            next_refresh: Instant::now(),
            last_signal: None,
        })
    }

    fn refresh(&mut self) -> Result<()> {
        let mut meters = Vec::new();
        unsafe {
            let devices = self
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            for index in 0..devices.GetCount()? {
                let Ok(device) = devices.Item(index) else {
                    continue;
                };
                let container = device
                    .OpenPropertyStore(STGM_READ)
                    .and_then(|store| store.GetValue(&PKEY_Device_ContainerId))
                    .and_then(|value| PropVariantToGUID(&value));
                if !container.is_ok_and(|id| self.containers.contains(&id)) {
                    continue;
                }
                if let Ok(meter) = device.Activate(CLSCTX_ALL, None) {
                    meters.push(meter);
                }
            }
        }
        self.meters = meters;
        Ok(())
    }

    pub fn is_playing(&mut self, source: Option<&str>) -> bool {
        if source != Some(self.local_address.as_str()) {
            self.last_signal = None;
            return false;
        }
        let now = Instant::now();
        if now >= self.next_refresh {
            self.meters.clear();
            let _ = self.refresh();
            self.next_refresh = now + Duration::from_secs(5);
        }
        let mut available = false;
        for meter in &self.meters {
            if let Ok(peak) = unsafe { meter.GetPeakValue() } {
                available = true;
                if peak > 0.00001 {
                    self.last_signal = Some(now);
                }
            }
        }
        if !available {
            self.last_signal = None;
        }
        self.last_signal
            .is_some_and(|time| now.duration_since(time) < Duration::from_millis(1500))
    }
}

fn headphone_containers(address: u64) -> Result<Vec<GUID>> {
    let mut length = 0;
    unsafe {
        ensure!(
            CM_Get_Device_ID_List_SizeW(&mut length, PCWSTR::null(), CM_GETIDLIST_FILTER_PRESENT)
                == CR_SUCCESS,
            "Audio device identities unavailable"
        );
        let mut devices = vec![0u16; length as usize];
        ensure!(
            CM_Get_Device_ID_ListW(PCWSTR::null(), &mut devices, CM_GETIDLIST_FILTER_PRESENT)
                == CR_SUCCESS,
            "Audio device identities changed"
        );
        let prefixes = [
            format!("BTHENUM\\DEV_{address:012X}\\"),
            format!("BTHLE\\DEV_{address:012X}\\"),
        ];
        let mut containers = Vec::new();
        for id in devices.split(|c| *c == 0).filter(|id| !id.is_empty()) {
            let id = String::from_utf16_lossy(id);
            if !prefixes
                .iter()
                .any(|prefix| id.to_ascii_uppercase().starts_with(prefix))
            {
                continue;
            }
            let mut node = 0;
            if CM_Locate_DevNodeW(&mut node, &HSTRING::from(id), CM_LOCATE_DEVNODE_NORMAL)
                != CR_SUCCESS
            {
                continue;
            }
            let mut container = GUID::zeroed();
            let mut kind = DEVPROPTYPE::default();
            let mut size = std::mem::size_of::<GUID>() as u32;
            if CM_Get_DevNode_PropertyW(
                node,
                &DEVPKEY_Device_ContainerId,
                &mut kind,
                Some((&mut container as *mut GUID).cast()),
                &mut size,
                0,
            ) == CR_SUCCESS
                && kind == DEVPROP_TYPE_GUID
                && size as usize == std::mem::size_of::<GUID>()
                && container != GUID::zeroed()
                && !containers.contains(&container)
            {
                containers.push(container);
            }
        }
        Ok(containers)
    }
}
