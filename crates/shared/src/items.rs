//! Item catalog for the extraction run. IDs are stable across client/server.

use crate::protocol::Item;

pub const FLASHLIGHT: u32 = 10;
pub const MAP: u32 = 11;
pub const PIPE_BAT: u32 = 12;
pub const CREDITS: u32 = 13;
pub const SCRAP: u32 = 14;
pub const MEDICAL_BAG: u32 = 15;
pub const HACKER_DEVICE: u32 = 16;
pub const SCRAP_PISTOL: u32 = 17;
pub const BANDAGE: u32 = 18;
pub const TAGGER: u32 = 19;
pub const ARMOR: u32 = 20;
pub const DATA_SHARD: u32 = 21;

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

pub fn scrap_pistol() -> Item {
    Item {
        id: SCRAP_PISTOL,
        name: "Scrap Pistol".into(),
        weight: 1.8,
        value: 60,
    }
}

pub fn bandage() -> Item {
    Item {
        id: BANDAGE,
        name: "Bandage".into(),
        weight: 0.1,
        value: 8,
    }
}

pub fn tagger() -> Item {
    Item {
        id: TAGGER,
        name: "Tagger".into(),
        weight: 0.5,
        value: 0,
    }
}

pub fn armor() -> Item {
    Item {
        id: ARMOR,
        name: "Body Armor".into(),
        weight: 4.0,
        value: 50,
    }
}

pub fn data_shard() -> Item {
    Item {
        id: DATA_SHARD,
        name: "Data Shard".into(),
        weight: 0.1,
        value: 35,
    }
}

/// Build an item from its id (spawn loadouts, shop grants, hack rewards).
pub fn by_id(id: u32) -> Option<Item> {
    Some(match id {
        FLASHLIGHT => flashlight(),
        MAP => map(),
        PIPE_BAT => pipe_bat(),
        MEDICAL_BAG => medical_bag(),
        HACKER_DEVICE => hacker_device(),
        SCRAP_PISTOL => scrap_pistol(),
        BANDAGE => bandage(),
        TAGGER => tagger(),
        ARMOR => armor(),
        DATA_SHARD => data_shard(),
        _ => return None,
    })
}

/// A consumable that restores health when used, and how much (base, before
/// class efficiency bonuses).
pub fn heal_amount(item: &Item) -> Option<f32> {
    match item.id {
        BANDAGE => Some(30.0),
        MEDICAL_BAG => Some(70.0),
        _ => None,
    }
}

pub fn is_armor(item: &Item) -> bool {
    item.id == ARMOR
}

/// What an NPC pays for an item — half list value, never zero. Lives in
/// shared so the trade window shows exactly what the server will credit.
pub fn sell_price(item: &Item) -> u32 {
    (item.value / 2).max(1)
}

pub fn is_gun(item: &Item) -> bool {
    item.id == SCRAP_PISTOL
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
