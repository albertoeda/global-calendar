//! Colour schemes. Each built-in theme maps its official palette onto a small set of roles.

use eframe::egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Ocean / canvas background.
    pub bg: Color32,
    /// Status bar and slider bar background.
    pub panel: Color32,
    pub land_day: Color32,
    pub land_night: Color32,
    pub border: Color32,
    pub text: Color32,
    pub subtext: Color32,
    /// Team markers, focus highlight, slider handle.
    pub accent: Color32,
    /// Time colour inside working hours.
    pub ok: Color32,
    /// Time colour at the edges of the day.
    pub warn: Color32,
    /// Time colour at night.
    pub bad: Color32,
    pub dark: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub palette: Palette,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[allow(clippy::too_many_arguments)]
const fn theme(
    id: &'static str,
    name: &'static str,
    dark: bool,
    [
        bg,
        panel,
        land_day,
        land_night,
        border,
        text,
        subtext,
        accent,
        ok,
        warn,
        bad,
    ]: [u32; 11],
) -> Theme {
    Theme {
        id,
        name,
        palette: Palette {
            bg: hex(bg),
            panel: hex(panel),
            land_day: hex(land_day),
            land_night: hex(land_night),
            border: hex(border),
            text: hex(text),
            subtext: hex(subtext),
            accent: hex(accent),
            ok: hex(ok),
            warn: hex(warn),
            bad: hex(bad),
            dark,
        },
    }
}

//                    bg        panel     day       night     border    text      subtext   accent    ok        warn      bad
#[rustfmt::skip]
pub const THEMES: &[Theme] = &[
    theme("catppuccin-mocha", "Catppuccin Mocha", true,
        [0x11111b, 0x181825, 0x585b70, 0x2a2b3c, 0x45475a, 0xcdd6f4, 0xa6adc8, 0xf9e2af, 0xa6e3a1, 0xfab387, 0xf38ba8]),
    theme("catppuccin-macchiato", "Catppuccin Macchiato", true,
        [0x181926, 0x1e2030, 0x5b6078, 0x2e3247, 0x494d64, 0xcad3f5, 0xa5adcb, 0xeed49f, 0xa6da95, 0xf5a97f, 0xed8796]),
    theme("catppuccin-frappe", "Catppuccin Frappé", true,
        [0x232634, 0x292c3c, 0x626880, 0x393d50, 0x51576d, 0xc6d0f5, 0xa5adce, 0xe5c890, 0xa6d189, 0xef9f76, 0xe78284]),
    theme("catppuccin-latte", "Catppuccin Latte", false,
        [0xeff1f5, 0xe6e9ef, 0x9ca0b0, 0xccd0da, 0xbcc0cc, 0x4c4f69, 0x6c6f85, 0xdf8e1d, 0x40a02b, 0xfe640b, 0xd20f39]),
    theme("gruvbox-dark", "Gruvbox Dark", true,
        [0x1d2021, 0x282828, 0x665c54, 0x32302f, 0x504945, 0xebdbb2, 0xa89984, 0xfabd2f, 0xb8bb26, 0xfe8019, 0xfb4934]),
    theme("gruvbox-light", "Gruvbox Light", false,
        [0xfbf1c7, 0xf2e5bc, 0xa89984, 0xd5c4a1, 0xbdae93, 0x3c3836, 0x7c6f64, 0xb57614, 0x79740e, 0xaf3a03, 0x9d0006]),
    theme("nord", "Nord", true,
        [0x2e3440, 0x3b4252, 0x616e88, 0x3b4252, 0x434c5e, 0xeceff4, 0x81a1c1, 0xebcb8b, 0xa3be8c, 0xd08770, 0xbf616a]),
    theme("dracula", "Dracula", true,
        [0x21222c, 0x282a36, 0x6272a4, 0x343746, 0x44475a, 0xf8f8f2, 0xa4a8c4, 0xf1fa8c, 0x50fa7b, 0xffb86c, 0xff5555]),
    theme("tokyo-night", "Tokyo Night", true,
        [0x16161e, 0x1a1b26, 0x414868, 0x24283b, 0x292e42, 0xc0caf5, 0xa9b1d6, 0xe0af68, 0x9ece6a, 0xff9e64, 0xf7768e]),
    theme("solarized-dark", "Solarized Dark", true,
        [0x002b36, 0x073642, 0x586e75, 0x0e4350, 0x586e75, 0x93a1a1, 0x839496, 0xb58900, 0x859900, 0xcb4b16, 0xdc322f]),
    theme("solarized-light", "Solarized Light", false,
        [0xfdf6e3, 0xeee8d5, 0x93a1a1, 0xe3dcc6, 0x93a1a1, 0x586e75, 0x657b83, 0xb58900, 0x859900, 0xcb4b16, 0xdc322f]),
    theme("one-dark", "One Dark", true,
        [0x21252b, 0x282c34, 0x5c6370, 0x353b45, 0x3b4048, 0xabb2bf, 0x7f848e, 0xe5c07b, 0x98c379, 0xd19a66, 0xe06c75]),
];

pub const DEFAULT_THEME: usize = 0;

/// Index of the theme with the given id, ignoring case and treating `_`/space like `-`.
pub fn find(id: &str) -> Option<usize> {
    let norm = |s: &str| {
        s.trim()
            .to_lowercase()
            .replace(['_', ' '], "-")
            .replace('é', "e")
    };
    let id = norm(id);
    THEMES.iter().position(|t| norm(t.id) == id)
}

pub fn next(index: usize, forward: bool) -> usize {
    let n = THEMES.len();
    if forward {
        (index + 1) % n
    } else {
        (index + n - 1) % n
    }
}

/// Linear blend from `a` (t = 0) to `b` (t = 1).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_findable() {
        for (i, t) in THEMES.iter().enumerate() {
            assert_eq!(find(t.id), Some(i));
        }
        assert_eq!(find("Catppuccin_Frappe"), find("catppuccin-frappe"));
        assert_eq!(find("Gruvbox Dark"), find("gruvbox-dark"));
        assert_eq!(find("nope"), None);
    }

    #[test]
    fn cycling_wraps() {
        assert_eq!(next(THEMES.len() - 1, true), 0);
        assert_eq!(next(0, false), THEMES.len() - 1);
    }

    #[test]
    fn mix_endpoints() {
        let (a, b) = (hex(0x000000), hex(0xffffff));
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 0.5), hex(0x808080));
    }
}
