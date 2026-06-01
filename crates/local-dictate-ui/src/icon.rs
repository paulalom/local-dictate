#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDictateIconData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn local_dictate_icon_data() -> LocalDictateIconData {
    let size = 32;
    let mut rgba = vec![0; size * size * 4];

    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) * 4;
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let in_disc = dx * dx + dy * dy <= 15 * 15;

            if in_disc {
                rgba[index] = 32;
                rgba[index + 1] = 88;
                rgba[index + 2] = 120;
                rgba[index + 3] = 255;
            }

            let mic_body = (12..=19).contains(&x) && (7..=19).contains(&y);
            let mic_stem = (15..=16).contains(&x) && (21..=25).contains(&y);
            let mic_base = (11..=20).contains(&x) && (25..=26).contains(&y);
            let mic_curve =
                (9..=22).contains(&x) && (17..=23).contains(&y) && !(12..=19).contains(&x);

            if mic_body || mic_stem || mic_base || mic_curve {
                rgba[index] = 248;
                rgba[index + 1] = 252;
                rgba[index + 2] = 255;
                rgba[index + 3] = 255;
            }
        }
    }

    LocalDictateIconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}
