//! Small pictures of the themed tables and halls for the shop's cards.

use macroquad::prelude::*;
use snookered_proto::shop::{HallStyle, TableStyle};

pub fn table(r: Rect, _style: TableStyle) {
    crate::ui::rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
}

pub fn hall(r: Rect, _style: HallStyle) {
    crate::ui::rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
}
