//! Units used in the register. Everything is converted to SI by the resolver;
//! the core only ever sees SI (m, s, m/s, …).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A register unit with its conversion to SI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Unit {
    /// Dimensionless (shares, factors, coefficients).
    #[serde(rename = "1")]
    Dimensionless,
    #[serde(rename = "m")]
    Metre,
    #[serde(rename = "cm")]
    Centimetre,
    #[serde(rename = "km")]
    Kilometre,
    #[serde(rename = "s")]
    Second,
    #[serde(rename = "min")]
    Minute,
    #[serde(rename = "h")]
    Hour,
    #[serde(rename = "day")]
    Day,
    #[serde(rename = "m/s")]
    MetrePerSecond,
    #[serde(rename = "km/h")]
    KilometrePerHour,
    #[serde(rename = "m/s2")]
    MetrePerSecondSquared,
    #[serde(rename = "rad")]
    Radian,
    /// Percent, stored as a fraction in SI (50 % → 0.5).
    #[serde(rename = "%")]
    Percent,
    #[serde(rename = "1/h")]
    PerHour,
    #[serde(rename = "1/km")]
    PerKilometre,
    #[serde(rename = "kW")]
    Kilowatt,
    #[serde(rename = "kg")]
    Kilogram,
    /// Clearance slope: centimetres per km/h of pair speed (→ seconds in SI).
    #[serde(rename = "cm/(km/h)")]
    CentimetrePerKmh,
    /// Speed lost per percent of upgrade (→ m/s per percent point).
    #[serde(rename = "(km/h)/%")]
    KmhPerPercent,
    /// Speed lost per unit of roughness, km/h per IRI (m/km) (→ m/s per IRI).
    #[serde(rename = "(km/h)/IRI")]
    KmhPerIri,
    /// Hour of the day, 0–24 (kept as hours).
    #[serde(rename = "hour_of_day")]
    HourOfDay,
}

impl Unit {
    /// Multiply a value in this unit by this factor to get SI.
    pub fn si_factor(self) -> f64 {
        match self {
            Unit::Dimensionless
            | Unit::Metre
            | Unit::Second
            | Unit::MetrePerSecond
            | Unit::MetrePerSecondSquared
            | Unit::Radian
            | Unit::Kilogram
            | Unit::HourOfDay => 1.0,
            Unit::Centimetre => 0.01,
            Unit::Kilometre => 1000.0,
            Unit::Minute => 60.0,
            Unit::Hour => 3600.0,
            Unit::Day => 86_400.0,
            Unit::KilometrePerHour | Unit::KmhPerPercent | Unit::KmhPerIri => 1.0 / 3.6,
            Unit::Percent => 0.01,
            Unit::PerHour => 1.0 / 3600.0,
            Unit::PerKilometre => 0.001,
            Unit::Kilowatt => 1000.0,
            Unit::CentimetrePerKmh => 0.01 * 3.6,
        }
    }

    /// The unit as written in the register.
    pub fn label(self) -> &'static str {
        match self {
            Unit::Dimensionless => "1",
            Unit::Metre => "m",
            Unit::Centimetre => "cm",
            Unit::Kilometre => "km",
            Unit::Second => "s",
            Unit::Minute => "min",
            Unit::Hour => "h",
            Unit::Day => "day",
            Unit::MetrePerSecond => "m/s",
            Unit::KilometrePerHour => "km/h",
            Unit::MetrePerSecondSquared => "m/s2",
            Unit::Radian => "rad",
            Unit::Percent => "%",
            Unit::PerHour => "1/h",
            Unit::PerKilometre => "1/km",
            Unit::Kilowatt => "kW",
            Unit::Kilogram => "kg",
            Unit::CentimetrePerKmh => "cm/(km/h)",
            Unit::KmhPerPercent => "(km/h)/%",
            Unit::KmhPerIri => "(km/h)/IRI",
            Unit::HourOfDay => "hour_of_day",
        }
    }

    /// The SI unit the value is converted to.
    pub fn si_label(self) -> &'static str {
        match self {
            Unit::Dimensionless | Unit::Percent => "1",
            Unit::Metre | Unit::Centimetre | Unit::Kilometre => "m",
            Unit::Second | Unit::Minute | Unit::Hour | Unit::Day | Unit::CentimetrePerKmh => "s",
            Unit::MetrePerSecond | Unit::KilometrePerHour => "m/s",
            Unit::MetrePerSecondSquared => "m/s2",
            Unit::Radian => "rad",
            Unit::PerHour => "1/s",
            Unit::PerKilometre => "1/m",
            Unit::Kilowatt => "W",
            Unit::Kilogram => "kg",
            Unit::KmhPerPercent => "(m/s)/%",
            Unit::KmhPerIri => "(m/s)/IRI",
            Unit::HourOfDay => "hour_of_day",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert!((36.0 * Unit::KilometrePerHour.si_factor() - 10.0).abs() < 1e-12);
        assert!((120.9 * Unit::Centimetre.si_factor() - 1.209).abs() < 1e-12);
        // 0.615 cm per km/h = 0.02214 m per m/s.
        assert!((0.615 * Unit::CentimetrePerKmh.si_factor() - 0.02214).abs() < 1e-12);
        assert!((3.0 * Unit::Day.si_factor() - 259_200.0).abs() < 1e-9);
    }

    #[test]
    fn labels_round_trip_through_serde() {
        for u in [
            Unit::KilometrePerHour,
            Unit::CentimetrePerKmh,
            Unit::Dimensionless,
            Unit::Percent,
        ] {
            let s = serde_json::to_string(&u).unwrap();
            assert_eq!(s, format!("\"{}\"", u.label()));
            let back: Unit = serde_json::from_str(&s).unwrap();
            assert_eq!(back, u);
        }
    }
}
