use std::collections::HashMap;

#[derive(Clone, PartialEq, Debug)]
pub enum Color {
    RGB(u8, u8, u8),
    Random,
    Default,
}

impl Color {
    pub const RED: Self = Color::RGB(255, 0, 0);
    pub const GREEN: Self = Color::RGB(0, 255, 0);
    pub const YELLOW: Self = Color::RGB(255, 255, 0);
    pub const BLUE: Self = Color::RGB(0, 0, 255);
    pub const MAGENTA: Self = Color::RGB(255, 0, 255);
    pub const CYAN: Self = Color::RGB(0, 255, 255);
    pub const WHITE: Self = Color::RGB(255, 255, 0);
    pub const BLACK: Self = Color::RGB(0, 0, 0);

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color::RGB(r, g, b)
    }

    pub fn true_random() -> Self {
        Color::RGB(rand::random(), rand::random(), rand::random())
    }

    pub fn random_color_list(num_cmds: u32) -> Vec<Self> {
        let mut colors = Vec::new();
        if num_cmds == 0 {
            return colors;
        }
        let mut start = rand::random::<u32>() % 360;
        let space = 360 / num_cmds;

        while colors.len() < num_cmds as usize {
            colors.push(theta_to_rgb(start));
            start += space;
            start %= 360;
        }

        colors
    }
}

pub fn open_sequence(color: &Color) -> String {
    match color {
        Color::RGB(r, g, b) => format!("\x1b[38;2;{};{};{}m", r, g, b),
        Color::Random => format!(
            "\x1b[38;2;{};{};{}m",
            rand::random::<u8>(),
            rand::random::<u8>(),
            rand::random::<u8>()
        ),
        Color::Default => close_sequence(),
    }
}

pub fn close_sequence() -> String {
    "\x1b[0m".to_string()
}

impl Default for Color {
    fn default() -> Self {
        Color::Default
    }
}

pub fn populate_random_colors(color_list: &mut HashMap<String, Color>) {
    let mut num_random = 0;

    for (_, color) in color_list.iter() {
        if color == &Color::Random {
            num_random += 1;
        }
    }

    let mut random_list = Color::random_color_list(num_random);

    for (_, color) in color_list.iter_mut() {
        if color == &Color::Random {
            *color = random_list.pop().unwrap();
        }
    }
}

fn theta_to_rgb(theta: u32) -> Color {
    let c = 1.0;
    let h_prime = f64::from(theta) / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());

    let (r_1, g_1, b_1) = if (0.0..1.0).contains(&h_prime) {
        (c, x, 0.0)
    } else if (1.0..2.0).contains(&h_prime) {
        (x, c, 0.0)
    } else if (2.0..3.0).contains(&h_prime) {
        (0.0, c, x)
    } else if (3.0..4.0).contains(&h_prime) {
        (0.0, x, c)
    } else if (4.0..5.0).contains(&h_prime) {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    Color::RGB(
        (r_1 * 255.0).round() as u8,
        (g_1 * 255.0).round() as u8,
        (b_1 * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {

    use super::theta_to_rgb;
    use super::Color;

    #[test]
    fn test_theta_to_rgb() {
        let red = theta_to_rgb(0);
        let green = theta_to_rgb(120);
        let blue = theta_to_rgb(240);
        let yellow = theta_to_rgb(60);
        let cyan = theta_to_rgb(180);
        let magenta = theta_to_rgb(300);

        let first_rand = theta_to_rgb(51);
        let second_rand = theta_to_rgb(95);
        let third_rand = theta_to_rgb(144);
        let fourth_rand = theta_to_rgb(202);
        let fifth_rand = theta_to_rgb(277);
        let sixth_rand = theta_to_rgb(354);

        assert_eq!(Color::RED, red);
        assert_eq!(Color::GREEN, green);
        assert_eq!(Color::BLUE, blue);

        assert_eq!(Color::YELLOW, yellow);
        assert_eq!(Color::CYAN, cyan);
        assert_eq!(Color::MAGENTA, magenta);

        assert_eq!(Color::RGB(255, 217, 0), first_rand);
        assert_eq!(Color::RGB(106, 255, 0), second_rand);
        assert_eq!(Color::RGB(0, 255, 102), third_rand);
        assert_eq!(Color::RGB(0, 162, 255), fourth_rand);
        assert_eq!(Color::RGB(157, 0, 255), fifth_rand);
        assert_eq!(Color::RGB(255, 0, 25), sixth_rand);
    }
}
