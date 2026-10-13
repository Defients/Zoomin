use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    pub hex: String,
    pub rgb: String,
    pub hsl: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        let (rf, gf, bf) = (r as f64 / 255., g as f64 / 255., b as f64 / 255.);
        let max = rf.max(gf).max(bf);
        let min = rf.min(gf).min(bf);
        let d = max - min;
        let l = (max + min) / 2.;
        let s = if d == 0. {
            0.
        } else {
            d / (1. - (2. * l - 1.).abs())
        };
        let h = if d == 0. {
            0.
        } else if max == rf {
            60. * ((gf - bf) / d).rem_euclid(6.)
        } else if max == gf {
            60. * ((bf - rf) / d + 2.)
        } else {
            60. * ((rf - gf) / d + 4.)
        };
        Self {
            hex: format!("#{r:02X}{g:02X}{b:02X}"),
            rgb: format!("rgb({r}, {g}, {b})"),
            hsl: format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s * 100., l * 100.),
            r,
            g,
            b,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversions() {
        assert_eq!(Color::new(255, 0, 0).hsl, "hsl(0, 100%, 50%)");
        assert_eq!(Color::new(128, 128, 128).hsl, "hsl(0, 0%, 50%)");
        assert_eq!(Color::new(0, 255, 0).hex, "#00FF00");
    }
}
