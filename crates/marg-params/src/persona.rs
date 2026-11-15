//! Driver personas and traits (G§3, ADR-0018 parts 5–6).
//!
//! A persona is a mean shift of the trait vector, not a rulebook; which
//! personas can drive which class of vehicle follows the "typical vehicle"
//! column of G§3.2. That mapping is structure, like the clearance groups: the
//! numbers (shares, shifts) live in the register.

use crate::vehicle::VehicleClass;

/// The ten starter personas of G§3.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Persona {
    AverageCommuter,
    RushedDriver,
    DeliveryRider,
    BikeTaxiRider,
    ERickshawDriver,
    AutoRickshawDriver,
    BusDriver,
    TruckLcvDriver,
    CautiousDriver,
    Cyclist,
}

impl Persona {
    pub const ALL: [Persona; 10] = [
        Persona::AverageCommuter,
        Persona::RushedDriver,
        Persona::DeliveryRider,
        Persona::BikeTaxiRider,
        Persona::ERickshawDriver,
        Persona::AutoRickshawDriver,
        Persona::BusDriver,
        Persona::TruckLcvDriver,
        Persona::CautiousDriver,
        Persona::Cyclist,
    ];

    /// Register key (`personas.shares.<key>`, `personas.shift.<key>.<trait>`).
    pub fn key(self) -> &'static str {
        match self {
            Persona::AverageCommuter => "average_commuter",
            Persona::RushedDriver => "rushed_driver",
            Persona::DeliveryRider => "delivery_rider",
            Persona::BikeTaxiRider => "bike_taxi_rider",
            Persona::ERickshawDriver => "e_rickshaw_driver",
            Persona::AutoRickshawDriver => "auto_rickshaw_driver",
            Persona::BusDriver => "bus_driver",
            Persona::TruckLcvDriver => "truck_lcv_driver",
            Persona::CautiousDriver => "cautious_driver",
            Persona::Cyclist => "cyclist",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap()
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| p.key() == key)
    }

    /// The personas that can drive a vehicle of `class`, the class's default
    /// first. From G§3.2's "typical vehicle" column, by class: a persona
    /// typical of one two-wheeler type (motorcycle, scooty, e-scooter) can
    /// ride any of them. Tractors have no persona in the guide; they take the
    /// goods-vehicle driver's (ADR-0018 part 6).
    pub fn eligible(class: VehicleClass) -> &'static [Persona] {
        use Persona::*;
        match class {
            VehicleClass::Bicycle | VehicleClass::CycleRickshaw => &[Cyclist],
            VehicleClass::TwoWheeler => {
                &[AverageCommuter, RushedDriver, DeliveryRider, BikeTaxiRider]
            }
            VehicleClass::ERickshaw => &[ERickshawDriver],
            VehicleClass::AutoRickshaw => &[AutoRickshawDriver],
            VehicleClass::Car => &[AverageCommuter, RushedDriver, CautiousDriver],
            VehicleClass::Lcv | VehicleClass::Truck | VehicleClass::Tractor => &[TruckLcvDriver],
            VehicleClass::Bus => &[BusDriver],
        }
    }
}

/// The trait dimensions every agent carries (G§3.1), in storage order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Trait {
    Aggressiveness,
    TimePressure,
    RiskTolerance,
    Conformity,
    NormSensitivity,
    Skill,
    Distraction,
}

impl Trait {
    pub const ALL: [Trait; 7] = [
        Trait::Aggressiveness,
        Trait::TimePressure,
        Trait::RiskTolerance,
        Trait::Conformity,
        Trait::NormSensitivity,
        Trait::Skill,
        Trait::Distraction,
    ];

    /// Register key (`personas.traits.<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Trait::Aggressiveness => "aggressiveness",
            Trait::TimePressure => "time_pressure",
            Trait::RiskTolerance => "risk_tolerance",
            Trait::Conformity => "conformity",
            Trait::NormSensitivity => "norm_sensitivity",
            Trait::Skill => "skill",
            Trait::Distraction => "distraction",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_persona_and_every_persona_a_class() {
        for c in VehicleClass::ALL {
            assert!(!Persona::eligible(c).is_empty(), "{}", c.key());
        }
        for p in Persona::ALL {
            assert!(
                VehicleClass::ALL
                    .iter()
                    .any(|&c| Persona::eligible(c).contains(&p)),
                "{} drives nothing",
                p.key()
            );
            assert_eq!(Persona::from_key(p.key()), Some(p));
            assert_eq!(Persona::ALL[p.index()], p);
        }
        for t in Trait::ALL {
            assert_eq!(Trait::ALL[t.index()], t);
        }
    }

    #[test]
    fn eligibility_follows_the_guide() {
        // G§3.2: delivery and bike-taxi riders are on two-wheelers, the
        // cautious driver in a car, every e-rickshaw has its own driver.
        let tw = Persona::eligible(VehicleClass::TwoWheeler);
        assert!(tw.contains(&Persona::DeliveryRider) && tw.contains(&Persona::BikeTaxiRider));
        assert!(!tw.contains(&Persona::CautiousDriver));
        assert!(Persona::eligible(VehicleClass::Car).contains(&Persona::CautiousDriver));
        assert_eq!(
            Persona::eligible(VehicleClass::ERickshaw),
            &[Persona::ERickshawDriver]
        );
        assert_eq!(Persona::eligible(VehicleClass::Bus), &[Persona::BusDriver]);
    }
}
