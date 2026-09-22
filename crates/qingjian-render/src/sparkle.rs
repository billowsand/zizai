//! 整句候选的小星标：四角星（上下左右四个尖、腰身向内收），实心填充，标在候选词右上角，
//! 表示这条是本地整句模型拼出来的，不是词库里现成的词。

use tiny_skia::{BlendMode, PathBuilder, Pixmap};

use crate::canvas::Canvas;
use crate::color::Color;

/// 腰身收进去的程度：控制点离中心的距离相对半径的比例，越小尖越细。
const WAIST_RATIO: f32 = 0.18;

/// 画在 `(x, y)` 为左上角、边长 `size` 像素的方块里。
pub(crate) fn draw_sparkle(canvas: &mut Canvas, x: f32, y: f32, size: f32, color: Color) {
    let side = size.ceil() as u32 + 1;
    let Some(icon) = Pixmap::new(side, side) else {
        return;
    };
    let mut layer = Canvas::from_pixmap(icon);
    if let Some(path) = sparkle_shape(size) {
        layer.fill_path(&path, color, BlendMode::SourceOver);
    }
    let icon = layer.into_pixmap();
    canvas.blend_pixmap(x.round() as i32, y.round() as i32, &icon);
}

/// 四角星轮廓：四个尖在边长中点，相邻两尖之间用一条向中心弯的二次曲线连起来。
fn sparkle_shape(size: f32) -> Option<tiny_skia::Path> {
    let c = size / 2.0;
    let r = size / 2.0;
    let w = r * WAIST_RATIO;
    let mut path = PathBuilder::new();
    path.move_to(c, c - r);
    path.quad_to(c + w, c - w, c + r, c);
    path.quad_to(c + w, c + w, c, c + r);
    path.quad_to(c - w, c + w, c - r, c);
    path.quad_to(c - w, c - w, c, c - r);
    path.close();
    path.finish()
}
