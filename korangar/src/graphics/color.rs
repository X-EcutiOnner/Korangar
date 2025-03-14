use std::ops::{Add, Mul, Sub};

use mlua::{Lua, Value};
use ragnarok_formats::color::{ColorBGRA, ColorRGB};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub red: f32,
    pub blue: f32,
    pub green: f32,
    pub alpha: f32,
}

impl Color {
    pub const BLACK: Self = Self::monochrome(0.0);
    pub const WHITE: Self = Self::monochrome(1.0);

    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self { red, green, blue, alpha }
    }

    pub const fn rgb_u8(red: u8, green: u8, blue: u8) -> Self {
        let red = (red as f32) / 255.0;
        let green = (green as f32) / 255.0;
        let blue = (blue as f32) / 255.0;

        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    pub const fn rgba_u8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        let red = (red as f32) / 255.0;
        let green = (green as f32) / 255.0;
        let blue = (blue as f32) / 255.0;
        let alpha = (alpha as f32) / 255.0;

        Self { red, green, blue, alpha }
    }

    pub fn rgb_hex(hex: &str) -> Self {
        assert_eq!(hex.len(), 6);

        let channel = |range| u8::from_str_radix(&hex[range], 16).unwrap();
        Color::rgb_u8(channel(0..2), channel(2..4), channel(4..6))
    }

    pub const fn monochrome(brightness: f32) -> Self {
        Self {
            red: brightness,
            green: brightness,
            blue: brightness,
            alpha: 1.0,
        }
    }

    pub fn monochrome_u8(brightness: u8) -> Self {
        let brightness = (brightness as f32) / 255.0;
        Self {
            red: brightness,
            green: brightness,
            blue: brightness,
            alpha: 1.0,
        }
    }

    pub fn red_as_u8(&self) -> u8 {
        (self.red * 255.0) as u8
    }

    pub fn green_as_u8(&self) -> u8 {
        (self.green * 255.0) as u8
    }

    pub fn blue_as_u8(&self) -> u8 {
        (self.blue * 255.0) as u8
    }

    pub fn alpha_as_u8(&self) -> u8 {
        (self.alpha * 255.0) as u8
    }

    #[cfg(feature = "debug")]
    pub const fn multiply_alpha(mut self, alpha: f32) -> Self {
        self.alpha *= alpha;
        self
    }

    pub const fn invert(&self) -> Self {
        Self::rgba(1.0 - self.red, 1.0 - self.blue, 1.0 - self.green, self.alpha)
    }

    pub fn shade(&self) -> Self {
        match (self.red_as_u8() as usize) + (self.green_as_u8() as usize) + (self.blue_as_u8() as usize) > 382 {
            true => Self::rgba_u8(
                self.red_as_u8().saturating_sub(40),
                self.green_as_u8().saturating_sub(40),
                self.blue_as_u8().saturating_sub(40),
                self.alpha_as_u8(),
            ),
            false => Self::rgba_u8(
                self.red_as_u8().saturating_add(40),
                self.green_as_u8().saturating_add(40),
                self.blue_as_u8().saturating_add(40),
                self.alpha_as_u8(),
            ),
        }
    }

    /// Converts the sRGB color into a linear representation for the shader.
    /// Since we use pre-multiplied alpha blending, we premultiply the alpha
    /// here too.
    pub fn components_linear(self) -> [f32; 4] {
        let srgb = [self.red, self.green, self.blue];
        let linear = srgb.map(|channel| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        [linear[0] * self.alpha, linear[1] * self.alpha, linear[2] * self.alpha, self.alpha]
    }
}

impl From<Color> for cosmic_text::Color {
    fn from(value: Color) -> Self {
        Self::rgba(value.red_as_u8(), value.green_as_u8(), value.blue_as_u8(), value.alpha_as_u8())
    }
}

impl From<cosmic_text::Color> for Color {
    fn from(value: cosmic_text::Color) -> Self {
        Self::rgba_u8(value.r(), value.g(), value.b(), value.a())
    }
}

impl Add for Color {
    type Output = Color;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            red: self.red + rhs.red,
            blue: self.blue + rhs.blue,
            green: self.green + rhs.green,
            alpha: self.alpha + rhs.alpha,
        }
    }
}

impl Sub for Color {
    type Output = Color;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            red: self.red - rhs.red,
            blue: self.blue - rhs.blue,
            green: self.green - rhs.green,
            alpha: self.alpha - rhs.alpha,
        }
    }
}

impl Mul<f32> for Color {
    type Output = Color;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            red: self.red * rhs,
            blue: self.blue * rhs,
            green: self.green * rhs,
            alpha: self.alpha * rhs,
        }
    }
}

impl From<Color> for [f32; 3] {
    fn from(val: Color) -> Self {
        [val.red, val.green, val.blue]
    }
}

impl From<Color> for [f32; 4] {
    fn from(val: Color) -> Self {
        [val.red, val.green, val.blue, val.alpha]
    }
}

impl From<ColorRGB> for Color {
    fn from(value: ColorRGB) -> Self {
        let ColorRGB { red, blue, green } = value;
        Color {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }
}

impl From<ColorBGRA> for Color {
    fn from(value: ColorBGRA) -> Self {
        let ColorBGRA { red, blue, green, alpha } = value;
        Color::rgba_u8(red, green, blue, alpha)
    }
}

impl mlua::FromLua for Color {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        if let Value::Table(table) = value {
            // Robust color parsing in case color values are not in u8 range.
            let mut sequence = table.sequence_values::<i64>();
            let r = i64::clamp(sequence.next().unwrap_or(Ok(0))?, 0, 255) as f32;
            let g = i64::clamp(sequence.next().unwrap_or(Ok(0))?, 0, 255) as f32;
            let b = i64::clamp(sequence.next().unwrap_or(Ok(0))?, 0, 255) as f32;
            return Ok(Color::rgb(r / 255.0, g / 255.0, b / 255.0));
        }

        Err(mlua::Error::FromLuaConversionError {
            from: "Table",
            to: "Color".to_string(),
            message: Some("Could not convert color".to_string()),
        })
    }
}
