//! Item catalog for the extraction run. IDs are stable across client/server.

use crate::protocol::Item;

pub const FLASHLIGHT: u32 = 10;
pub const MAP: u32 = 11;
pub const PIPE_BAT: u32 = 12;
pub const CREDITS: u32 = 13;
pub const SCRAP: u32 = 14;
pub const MEDICAL_BAG: u32 = 15;
pub const HACKER_DEVICE: u32 = 16;

pub fn flashlight() -> Item {
    Item {
        id: FLASHLIGHT,
        name: "Flashlight".into(),
        weight: 0.8,
        value: 25,
    }
}

pub fn map() -> Item {
    Item {
        id: MAP,
        name: "Sector Map".into(),
        weight: 0.3,
        value: 40,
    }
}

pub fn pipe_bat() -> Item {
    Item {
        id: PIPE_BAT,
        name: "Pipe Bat".into(),
        weight: 2.5,
        value: 15,
    }
}

pub fn credits(amount: u32) -> Item {
    Item {
        id: CREDITS,
        name: format!("{amount} Credits"),
        weight: 0.0,
        value: amount,
    }
}

pub fn scrap(amount: u32) -> Item {
    Item {
        id: SCRAP,
        name: format!("{amount} Scrap"),
        weight: 0.5,
        value: amount,
    }
}

pub fn medical_bag() -> Item {
    Item {
        id: MEDICAL_BAG,
        name: "Medical Bag".into(),
        weight: 1.5,
        value: 0,
    }
}

pub fn hacker_device() -> Item {
    Item {
        id: HACKER_DEVICE,
        name: "Hacker Device".into(),
        weight: 0.6,
        value: 0,
    }
}

pub fn is_map(item: &Item) -> bool {
    item.id == MAP
}

pub fn is_bat(item: &Item) -> bool {
    item.id == PIPE_BAT
}

pub fn is_flashlight(item: &Item) -> bool {
    item.id == FLASHLIGHT
}
