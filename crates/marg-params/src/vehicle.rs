//! Vehicle types and classes (G§1, plan/03-model-design.md §1).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Vehicle class: the level at which most interaction parameters are given.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VehicleClass {
    Bicycle,
    CycleRickshaw,
    TwoWheeler,
    ERickshaw,
    AutoRickshaw,
    Car,
    Lcv,
    Bus,
    Truck,
    Tractor,
}

/// The vehicle groups of Budhkar & Maurya (2017), whose lateral clearance
/// lines are given per pair of groups (ADR-0010 §1). Classes the paper did not
/// observe borrow the nearest group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClearanceGroup {
    TwoWheeler,
    ThreeWheeler,
    Car,
    Lcv,
    Heavy,
}

impl ClearanceGroup {
    /// The register key, e.g. `three_wheeler`.
    pub fn key(self) -> &'static str {
        match self {
            ClearanceGroup::TwoWheeler => "two_wheeler",
            ClearanceGroup::ThreeWheeler => "three_wheeler",
            ClearanceGroup::Car => "car",
            ClearanceGroup::Lcv => "lcv",
            ClearanceGroup::Heavy => "heavy",
        }
    }
}

impl VehicleClass {
    /// The clearance group of this class: bicycles count as two-wheelers,
    /// cycle- and e-rickshaws as three-wheelers ("autos"), and buses, trucks
    /// and tractors as heavy vehicles.
    pub fn clearance_group(self) -> ClearanceGroup {
        match self {
            VehicleClass::Bicycle | VehicleClass::TwoWheeler => ClearanceGroup::TwoWheeler,
            VehicleClass::CycleRickshaw | VehicleClass::ERickshaw | VehicleClass::AutoRickshaw => {
                ClearanceGroup::ThreeWheeler
            }
            VehicleClass::Car => ClearanceGroup::Car,
            VehicleClass::Lcv => ClearanceGroup::Lcv,
            VehicleClass::Bus | VehicleClass::Truck | VehicleClass::Tractor => {
                ClearanceGroup::Heavy
            }
        }
    }
}

/// Vehicle type (sub-class): the level at which dimensions and desired speeds
/// are drawn. 15 types, above the guide's minimum of 12.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VehicleType {
    Bicycle,
    CycleRickshaw,
    Moped,
    Scooter,
    Motorcycle,
    ETwoWheeler,
    ERickshaw,
    AutoRickshaw,
    Hatchback,
    Sedan,
    Suv,
    Lcv,
    Bus,
    Truck,
    Tractor,
}

impl VehicleClass {
    /// All classes in a fixed order.
    pub const ALL: [VehicleClass; 10] = [
        VehicleClass::Bicycle,
        VehicleClass::CycleRickshaw,
        VehicleClass::TwoWheeler,
        VehicleClass::ERickshaw,
        VehicleClass::AutoRickshaw,
        VehicleClass::Car,
        VehicleClass::Lcv,
        VehicleClass::Bus,
        VehicleClass::Truck,
        VehicleClass::Tractor,
    ];

    /// The register key, e.g. `two_wheeler`.
    pub fn key(self) -> &'static str {
        match self {
            VehicleClass::Bicycle => "bicycle",
            VehicleClass::CycleRickshaw => "cycle_rickshaw",
            VehicleClass::TwoWheeler => "two_wheeler",
            VehicleClass::ERickshaw => "e_rickshaw",
            VehicleClass::AutoRickshaw => "auto_rickshaw",
            VehicleClass::Car => "car",
            VehicleClass::Lcv => "lcv",
            VehicleClass::Bus => "bus",
            VehicleClass::Truck => "truck",
            VehicleClass::Tractor => "tractor",
        }
    }

    /// Index in [`VehicleClass::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

impl VehicleType {
    /// All types in a fixed order.
    pub const ALL: [VehicleType; 15] = [
        VehicleType::Bicycle,
        VehicleType::CycleRickshaw,
        VehicleType::Moped,
        VehicleType::Scooter,
        VehicleType::Motorcycle,
        VehicleType::ETwoWheeler,
        VehicleType::ERickshaw,
        VehicleType::AutoRickshaw,
        VehicleType::Hatchback,
        VehicleType::Sedan,
        VehicleType::Suv,
        VehicleType::Lcv,
        VehicleType::Bus,
        VehicleType::Truck,
        VehicleType::Tractor,
    ];

    /// The class this type belongs to.
    pub fn class(self) -> VehicleClass {
        match self {
            VehicleType::Bicycle => VehicleClass::Bicycle,
            VehicleType::CycleRickshaw => VehicleClass::CycleRickshaw,
            VehicleType::Moped
            | VehicleType::Scooter
            | VehicleType::Motorcycle
            | VehicleType::ETwoWheeler => VehicleClass::TwoWheeler,
            VehicleType::ERickshaw => VehicleClass::ERickshaw,
            VehicleType::AutoRickshaw => VehicleClass::AutoRickshaw,
            VehicleType::Hatchback | VehicleType::Sedan | VehicleType::Suv => VehicleClass::Car,
            VehicleType::Lcv => VehicleClass::Lcv,
            VehicleType::Bus => VehicleClass::Bus,
            VehicleType::Truck => VehicleClass::Truck,
            VehicleType::Tractor => VehicleClass::Tractor,
        }
    }

    /// The register key, e.g. `motorcycle`.
    pub fn key(self) -> &'static str {
        match self {
            VehicleType::Bicycle => "bicycle",
            VehicleType::CycleRickshaw => "cycle_rickshaw",
            VehicleType::Moped => "moped",
            VehicleType::Scooter => "scooter",
            VehicleType::Motorcycle => "motorcycle",
            VehicleType::ETwoWheeler => "e_two_wheeler",
            VehicleType::ERickshaw => "e_rickshaw",
            VehicleType::AutoRickshaw => "auto_rickshaw",
            VehicleType::Hatchback => "hatchback",
            VehicleType::Sedan => "sedan",
            VehicleType::Suv => "suv",
            VehicleType::Lcv => "lcv",
            VehicleType::Bus => "bus",
            VehicleType::Truck => "truck",
            VehicleType::Tractor => "tractor",
        }
    }

    /// Index in [`VehicleType::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Parse a register key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_match_order() {
        for (i, t) in VehicleType::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
            assert_eq!(VehicleType::from_key(t.key()), Some(*t));
        }
        for (i, c) in VehicleClass::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
    }
}
