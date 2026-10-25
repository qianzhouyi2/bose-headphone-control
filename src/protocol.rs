use anyhow::{ensure, Result};
#[derive(Debug)]
pub struct DeviceRejected {
    pub block: u8,
    pub function: u8,
    pub code: u8,
}
impl std::fmt::Display for DeviceRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Headphones rejected [{}.{}, code {}]",
            self.block, self.function, self.code
        )
    }
}
impl std::error::Error for DeviceRejected {}
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub block: u8,
    pub function: u8,
    pub op: u8,
    pub payload: Vec<u8>,
}
pub fn packet(block: u8, function: u8, op: u8, payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(payload.len() <= 255, "BMAP payload too long");
    let mut out = vec![block, function, op & 15, payload.len() as u8];
    out.extend_from_slice(payload);
    Ok(out)
}
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}
impl Decoder {
    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Frame>> {
        ensure!(
            self.pending.len() + data.len() <= 65536,
            "BMAP notification overflow"
        );
        self.pending.extend_from_slice(data);
        let mut result = vec![];
        let mut pos = 0;
        while self.pending.len() - pos >= 4 {
            let end = pos + 4 + self.pending[pos + 3] as usize;
            if end > self.pending.len() {
                break;
            }
            result.push(Frame {
                block: self.pending[pos],
                function: self.pending[pos + 1],
                op: self.pending[pos + 2] & 15,
                payload: self.pending[pos + 4..end].to_vec(),
            });
            pos = end;
        }
        self.pending.drain(..pos);
        Ok(result)
    }
}
pub fn encode(packet: &[u8], mtu: usize) -> Result<Vec<Vec<u8>>> {
    ensure!(mtu >= 23, "Invalid BLE MTU");
    let size = mtu - 4;
    let count = packet.len().div_ceil(size).max(1);
    ensure!(count <= 16, "BLE segment limit");
    let mut out = vec![];
    for i in 0..count {
        let mut v = vec![(((count - 1) << 4) | i) as u8];
        v.extend_from_slice(
            &packet[(i * size).min(packet.len())..((i + 1) * size).min(packet.len())],
        );
        out.push(v);
    }
    Ok(out)
}
#[derive(Default)]
pub struct Framing {
    segments: Option<Vec<Option<Vec<u8>>>>,
}
impl Framing {
    pub fn push(&mut self, v: &[u8]) -> Result<Option<Vec<u8>>> {
        ensure!(!v.is_empty(), "Empty BLE notification");
        let last = (v[0] >> 4) as usize;
        let index = (v[0] & 15) as usize;
        ensure!(index <= last, "Invalid segment index");
        if last == 0 {
            ensure!(self.segments.is_none(), "Missing BLE segment");
            return Ok(Some(v[1..].to_vec()));
        }
        if self.segments.is_none() {
            ensure!(index == 0, "Missing first BLE segment");
            self.segments = Some(vec![None; last + 1]);
        }
        let slots = self.segments.as_mut().unwrap();
        ensure!(
            slots.len() == last + 1 && slots[index].is_none(),
            "Duplicate or inconsistent segment"
        );
        slots[index] = Some(v[1..].to_vec());
        if slots.iter().any(Option::is_none) {
            return Ok(None);
        }
        Ok(Some(
            self.segments
                .take()
                .unwrap()
                .into_iter()
                .flatten()
                .flatten()
                .collect(),
        ))
    }
}
pub fn status(frame: Frame, min: usize) -> Result<Vec<u8>> {
    if frame.op == 4 {
        return Err(DeviceRejected {
            block: frame.block,
            function: frame.function,
            code: frame.payload.first().copied().unwrap_or(0),
        }
        .into());
    }
    ensure!(
        matches!(frame.op, 3 | 6) && frame.payload.len() >= min,
        "Incomplete device reply [{}.{}]",
        frame.block,
        frame.function
    );
    Ok(frame.payload)
}
pub fn parse_eq(p: &[u8]) -> Result<Vec<crate::model::Band>> {
    let (bands, remainder) = p.as_chunks::<4>();
    ensure!(
        !bands.is_empty() && remainder.is_empty(),
        "Invalid EQ response"
    );
    bands
        .iter()
        .map(|b| {
            let (min, max, value) = (b[0] as i8 as i32, b[1] as i8 as i32, b[2] as i8 as i32);
            ensure!(min <= value && value <= max, "Invalid EQ range");
            Ok(crate::model::Band {
                id: b[3],
                min,
                max,
                value,
            })
        })
        .collect()
}
pub fn mac(p: &[u8]) -> String {
    p.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
pub fn mac_bytes(s: &str) -> Result<Vec<u8>> {
    let result = s
        .split(':')
        .map(|v| u8::from_str_radix(v, 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(result.len() == 6, "Invalid Bluetooth address");
    Ok(result)
}
