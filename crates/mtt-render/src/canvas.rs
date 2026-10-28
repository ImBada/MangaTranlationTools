//! Minimal Canvas2D-like wrapper over a tiny-skia pixmap: a transform stack with the same
//! pre-multiplication order as `CanvasRenderingContext2D` (`translate`/`rotate`/`transform`
//! apply to coordinates before the current transform).

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Shader, Stroke,
    Transform,
};

pub struct Canvas {
    pub pixmap: Pixmap,
    pub transform: Transform,
    stack: Vec<Transform>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            pixmap: Pixmap::new(width.max(1), height.max(1))?,
            transform: Transform::identity(),
            stack: Vec::new(),
        })
    }

    pub fn from_pixmap(pixmap: Pixmap) -> Self {
        Self { pixmap, transform: Transform::identity(), stack: Vec::new() }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn save(&mut self) {
        self.stack.push(self.transform);
    }

    pub fn restore(&mut self) {
        if let Some(transform) = self.stack.pop() {
            self.transform = transform;
        }
    }

    pub fn translate(&mut self, x: f64, y: f64) {
        self.transform = self.transform.pre_translate(x as f32, y as f32);
    }

    pub fn scale(&mut self, factor: f64) {
        self.transform = self.transform.pre_scale(factor as f32, factor as f32);
    }

    pub fn rotate_deg(&mut self, degrees: f64) {
        self.transform = self.transform.pre_concat(Transform::from_rotate(degrees as f32));
    }

    /// `context.transform(1, 0, skew, 1, 0, 0)`.
    pub fn skew_x(&mut self, skew: f64) {
        self.transform = self.transform.pre_concat(Transform::from_row(1.0, 0.0, skew as f32, 1.0, 0.0, 0.0));
    }

    pub fn fill_path(&mut self, path: &Path, shader: Shader<'_>) {
        let paint = Paint { shader, anti_alias: true, ..Paint::default() };
        self.pixmap.fill_path(path, &paint, FillRule::Winding, self.transform, None);
    }

    pub fn stroke_path(&mut self, path: &Path, width: f64, shader: Shader<'_>, cap: LineCap) {
        if width <= 0.0 {
            return;
        }
        let paint = Paint { shader, anti_alias: true, ..Paint::default() };
        let stroke = Stroke { width: width as f32, line_join: LineJoin::Round, line_cap: cap, ..Stroke::default() };
        self.pixmap.stroke_path(path, &paint, &stroke, self.transform, None);
    }

    pub fn fill_rect(&mut self, left: f64, top: f64, width: f64, height: f64, color: Color) {
        if let Some(rect) = Rect::from_xywh(left as f32, top as f32, width as f32, height as f32) {
            let paint = Paint { shader: Shader::SolidColor(color), anti_alias: true, ..Paint::default() };
            self.pixmap.fill_rect(rect, &paint, self.transform, None);
        }
    }

    /// Draws `source` with its top-left at device pixel `(x, y)`, ignoring the transform.
    pub fn draw_pixmap_device(&mut self, source: &Pixmap, x: i32, y: i32, opacity: f32, mask: Option<&Mask>) {
        self.pixmap.draw_pixmap(
            x,
            y,
            source.as_ref(),
            &PixmapPaint { opacity, ..PixmapPaint::default() },
            Transform::identity(),
            mask,
        );
    }
}

pub fn solid(color: Color) -> Shader<'static> {
    Shader::SolidColor(color)
}

/// `#rrggbb` + alpha → color; invalid hex falls back to black like a failed canvas parse
/// would keep the previous (default black) style.
pub fn hex_color(hex: &str, alpha: f64) -> Color {
    let (r, g, b) = mtt_core::geometry::parse_hex_color(hex).unwrap_or((0, 0, 0));
    Color::from_rgba8(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Rounded rectangle built with quadratic corners like `fillRoundRect`.
pub fn round_rect_path(left: f64, top: f64, width: f64, height: f64, radius: f64) -> Option<Path> {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0) as f32;
    let (l, t, w, h) = (left as f32, top as f32, width as f32, height as f32);
    if radius <= 0.0 {
        return Rect::from_xywh(l, t, w, h).map(PathBuilder::from_rect);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(l + w - radius, t);
    pb.quad_to(l + w, t, l + w, t + radius);
    pb.line_to(l + w, t + h - radius);
    pb.quad_to(l + w, t + h, l + w - radius, t + h);
    pb.line_to(l + radius, t + h);
    pb.quad_to(l, t + h, l, t + h - radius);
    pb.line_to(l, t + radius);
    pb.quad_to(l, t, l + radius, t);
    pb.close();
    pb.finish()
}

pub fn circles_path(centers: &[(f64, f64)], radius: f64) -> Option<Path> {
    if radius <= 0.0 {
        return None;
    }
    let mut pb = PathBuilder::new();
    for (x, y) in centers {
        pb.push_circle(*x as f32, *y as f32, radius as f32);
    }
    pb.finish()
}

/// Converts tiny-skia's premultiplied pixels to straight RGBA (`getImageData`).
pub fn to_straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = pixmap.data().to_vec();
    for p in out.chunks_exact_mut(4) {
        let alpha = p[3];
        if alpha != 255 {
            let color = tiny_skia::PremultipliedColorU8::from_rgba(p[0], p[1], p[2], alpha)
                .expect("premultiplied")
                .demultiply();
            p.copy_from_slice(&[color.red(), color.green(), color.blue(), alpha]);
        }
    }
    out
}

/// Writes straight RGBA back into a premultiplied pixmap (`putImageData`).
pub fn from_straight_rgba(pixmap: &mut Pixmap, data: &[u8]) {
    for (pixel, rgba) in pixmap.pixels_mut().iter_mut().zip(data.chunks_exact(4)) {
        *pixel = tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).premultiply();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_applies_before_existing_transform() {
        let mut canvas = Canvas::new(10, 10).unwrap();
        canvas.translate(5.0, 0.0);
        canvas.rotate_deg(90.0);
        let mut point = [tiny_skia::Point::from_xy(1.0, 0.0)];
        canvas.transform.map_points(&mut point);
        // rotate first (1,0) -> (0,1), then translate -> (5,1)
        assert!((point[0].x - 5.0).abs() < 1e-5 && (point[0].y - 1.0).abs() < 1e-5, "{:?}", point[0]);
    }

    #[test]
    fn straight_rgba_round_trip() {
        let mut pixmap = Pixmap::new(1, 1).unwrap();
        from_straight_rgba(&mut pixmap, &[200, 100, 50, 128]);
        let back = to_straight_rgba(&pixmap);
        assert!(back[0].abs_diff(200) <= 2 && back[1].abs_diff(100) <= 2 && back[3] == 128, "{back:?}");
    }
}
