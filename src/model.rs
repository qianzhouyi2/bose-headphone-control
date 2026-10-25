use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

pub const MIN_FAVORITE_MODES: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Band {
    pub id: u8,
    pub min: i32,
    pub max: i32,
    pub value: i32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Paired {
    pub mac: String,
    pub name: String,
    pub connected: bool,
}
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Playback {
    Stopped,
    Playing,
    Paused,
    Seeking,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DeviceAction {
    Connect,
    Disconnect,
}
#[derive(Copy, Clone, Debug)]
pub enum ConnectivitySetting {
    Multipoint,
    LeAudio,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub connected: bool,
    pub name: String,
    pub firmware: String,
    pub product_id: Option<u16>,
    #[serde(skip)]
    pub product_guid: String,
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    pub mode: u8,
    pub cnc: i32,
    pub spatial: u8,
    pub wind: bool,
    pub anc: bool,
    pub eq: Vec<Band>,
    pub modes: BTreeMap<u8, String>,
    pub favorites: u8,
    pub total_modes: u8,
    pub multipoint: Option<bool>,
    pub le_audio: Option<bool>,
    pub pause: Option<bool>,
    pub answer: Option<bool>,
    pub sidetone: Option<u8>,
    pub devices: Vec<Paired>,
    pub active: Option<String>,
    pub playback: Option<Playback>,
    pub protocol: String,
    pub transport: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            connected: false,
            name: String::new(),
            firmware: String::new(),
            product_id: None,
            product_guid: String::new(),
            battery: None,
            charging: None,
            mode: 255,
            cnc: 0,
            spatial: 0,
            wind: false,
            anc: false,
            eq: vec![],
            modes: [(0, "Quiet"), (1, "Aware"), (2, "Immersion"), (3, "Cinema")]
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            favorites: 0,
            total_modes: 11,
            multipoint: None,
            le_audio: None,
            pause: None,
            answer: None,
            sidetone: None,
            devices: vec![],
            active: None,
            playback: None,
            protocol: String::new(),
            transport: String::new(),
        }
    }
}
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SliderKey {
    Cnc,
    Eq(u8),
}
#[derive(Copy, Clone, Debug)]
pub struct Edit {
    pub key: SliderKey,
    pub value: i32,
    pub revision: u64,
    pub session: u64,
}
#[derive(Clone, Debug)]
pub enum Command {
    Slider(Edit),
    ResetEq(Vec<Edit>),
    Mode(u8),
    Spatial(u8),
    Wind(bool),
    Connectivity {
        setting: ConnectivitySetting,
        enabled: bool,
        revision: u64,
    },
    Pause(bool),
    Answer(bool),
    Sidetone(u8),
    Favorite(u8),
    ConnectDevice(String),
    DisconnectDevice(String),
    Refresh,
    Rename(String),
    Pair,
    Power,
    Reconnect,
    Quit,
}
#[derive(Clone, Debug)]
pub enum Event {
    Connecting,
    State(Box<State>),
    Ack(Edit, i32),
    ResetAck(Vec<Edit>),
    DeviceAck(String, DeviceAction),
    DeviceFailed(String, DeviceAction),
    ConnectivityDone(u64),
    Failed(Option<Edit>, String),
    Rejected(Option<Edit>),
    Disconnected(String),
    Show,
    Quit,
}

pub struct Queue {
    pub commands: VecDeque<(Command, Instant)>,
    pub stopped: bool,
    pub connecting: bool,
}
impl Queue {
    pub fn new() -> Self {
        Self {
            commands: VecDeque::new(),
            stopped: false,
            connecting: true,
        }
    }
    pub fn push(&mut self, command: Command, now: Instant, immediate: bool) {
        match &command {
            Command::Slider(edit) => self
                .commands
                .retain(|(c, _)| !matches!(c,Command::Slider(old) if old.key==edit.key)),
            Command::ResetEq(_) => self.commands.retain(|(c, _)| {
                !matches!(
                    c,
                    Command::Slider(Edit {
                        key: SliderKey::Eq(_),
                        ..
                    })
                )
            }),
            Command::Reconnect => self.commands.retain(|(c, _)| matches!(c, Command::Quit)),
            _ => {}
        }
        let due = if matches!(command, Command::Slider(_)) && !immediate {
            now + Duration::from_millis(120)
        } else {
            now
        };
        self.commands.push_back((command, due));
    }
    pub fn pop_ready(&mut self, now: Instant) -> Option<Command> {
        // Preserve barriers and command order; replace only unsent values.
        if self.commands.front().is_some_and(|(_, due)| *due <= now) {
            self.commands.pop_front().map(|(c, _)| c)
        } else {
            None
        }
    }
    pub fn clear_device_edits(&mut self) {
        self.commands
            .retain(|(c, _)| matches!(c, Command::Quit | Command::Reconnect));
    }
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub value: i32,
    pub revision: u64,
    pub session: u64,
}
#[derive(Default)]
pub struct Drafts {
    pub values: BTreeMap<SliderKey, Draft>,
    revision: u64,
    pub session: u64,
}
impl Drafts {
    pub fn edit(&mut self, key: SliderKey, value: i32) -> Edit {
        self.revision += 1;
        let e = Edit {
            key,
            value,
            revision: self.revision,
            session: self.session,
        };
        self.values.insert(
            key,
            Draft {
                value,
                revision: e.revision,
                session: e.session,
            },
        );
        e
    }
    pub fn complete(&mut self, e: Edit) -> bool {
        if self
            .values
            .get(&e.key)
            .is_some_and(|d| d.revision == e.revision && d.session == e.session)
        {
            self.values.remove(&e.key);
            true
        } else {
            false
        }
    }
    pub fn value(&self, key: SliderKey, confirmed: i32) -> i32 {
        self.values.get(&key).map_or(confirmed, |d| d.value)
    }
    pub fn reset_eq(&mut self) {
        self.values.retain(|key, _| *key == SliderKey::Cnc);
        self.revision += 1;
    }
    pub fn disconnect(&mut self) {
        self.values.clear();
        self.session += 1;
    }
}
