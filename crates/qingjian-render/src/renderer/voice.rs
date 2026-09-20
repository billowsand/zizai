//! 与候选窗同一套主题的语音面板。

use crate::canvas::Canvas;
use crate::error::RenderError;
use crate::shadow::Shadow;
use crate::text::TextStyle;
use crate::{Rendered, Theme, VoiceFrame, VoiceTone};

use super::{Metrics, Renderer};

const WIDTH: f32 = 360.0;
const WAVE_WIDTH: f32 = 96.0;
const WAVE_HEIGHT: f32 = 22.0;
const BAR_WIDTH: f32 = 3.0;
const BAR_GAP: f32 = 3.0;
const ROW_GAP: f32 = 5.0;
const TITLE_GAP: f32 = 12.0;
const STATUS_SIZE: f32 = 6.0;
const STATUS_GAP: f32 = 7.0;

impl Renderer {
    /// 画语音态；尺寸与候选窗一样以点为单位并共用阴影、圆角、字体和配色。
    pub fn render_voice(
        &mut self,
        frame: &VoiceFrame,
        theme: &Theme,
        scale: f32,
        shadow: Option<&Shadow>,
    ) -> Result<Rendered, RenderError> {
        let metrics = Metrics { theme, scale };
        let content_width = metrics.px(WIDTH);
        let top_height = metrics
            .px(WAVE_HEIGHT)
            .max(metrics.px(theme.text_font.line_height));
        let body_height = metrics.px(theme.annotation_font.line_height);
        let content_height =
            metrics.padding() * 2.0 + top_height + metrics.px(ROW_GAP) + body_height;
        let margin = shadow.map_or(0.0, |value| metrics.px(value.margin()));
        let mut canvas = Canvas::new(
            (content_width + margin * 2.0).ceil() as u32,
            (content_height + margin * 2.0).ceil() as u32,
        )?;
        let radius = metrics.corner_radius();
        if let Some(shadow) = shadow
            && let Some(content) =
                tiny_skia::Rect::from_xywh(margin, margin, content_width, content_height)
        {
            shadow.paint(&mut canvas, content, radius, scale);
        }
        canvas.fill_round_rect(
            margin,
            margin,
            content_width,
            content_height,
            radius,
            theme.colors.background,
        );

        let left = margin + metrics.padding();
        let top = margin + metrics.padding();
        draw_waveform(&mut canvas, frame, &metrics, left, top, top_height);

        let title_style = metrics.text_style();
        let status_x = left + metrics.px(WAVE_WIDTH + TITLE_GAP);
        let status_size = metrics.px(STATUS_SIZE);
        canvas.fill_round_rect(
            status_x,
            top + (top_height - status_size) / 2.0,
            status_size,
            status_size,
            status_size / 2.0,
            tone_color(frame.tone, theme),
        );
        let title_x = status_x + status_size + metrics.px(STATUS_GAP);
        let title_y = top + (top_height - title_style.line_height) / 2.0;
        self.draw_text(&mut canvas, &frame.title, &title_style, title_x, title_y);

        if let Some(elapsed) = &frame.elapsed {
            let elapsed_style = metrics.style(theme.annotation_font, theme.colors.gloss);
            let elapsed_width = self.measure(elapsed, &elapsed_style).width;
            let elapsed_x = margin + content_width - metrics.padding() - elapsed_width;
            let elapsed_y = top + (top_height - elapsed_style.line_height) / 2.0;
            self.draw_text(&mut canvas, elapsed, &elapsed_style, elapsed_x, elapsed_y);
        }

        let body = frame.transcript.as_deref().unwrap_or(&frame.hint);
        let body_color = if frame.transcript.is_some() {
            theme.colors.text
        } else {
            theme.colors.gloss
        };
        let body_style = metrics.style(theme.annotation_font, body_color);
        let body_y = top + top_height + metrics.px(ROW_GAP);
        let available = content_width - metrics.padding() * 2.0;
        let fitted = self.fit_tail(body, &body_style, available);
        self.draw_text(&mut canvas, &fitted, &body_style, left, body_y);

        Ok(Rendered {
            pixmap: canvas.into_pixmap(),
            content_x: margin as u32,
            content_y: margin as u32,
            content_width: content_width.ceil() as u32,
            content_height: content_height.ceil() as u32,
            scale,
        })
    }

    fn fit_tail(&mut self, text: &str, style: &TextStyle, available: f32) -> String {
        if self.measure(text, style).width <= available {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        for start in 1..chars.len() {
            let candidate = format!("…{}", chars[start..].iter().collect::<String>());
            if self.measure(&candidate, style).width <= available {
                return candidate;
            }
        }
        "…".to_owned()
    }
}

fn draw_waveform(
    canvas: &mut Canvas,
    frame: &VoiceFrame,
    metrics: &Metrics<'_>,
    left: f32,
    top: f32,
    row_height: f32,
) {
    let count = (WAVE_WIDTH / (BAR_WIDTH + BAR_GAP)).floor() as usize;
    let start = frame.levels.len().saturating_sub(count);
    let levels = &frame.levels[start..];
    let missing = count.saturating_sub(levels.len());
    for index in 0..count {
        let level = if index < missing {
            0
        } else {
            levels[index - missing].min(1000)
        };
        let ratio = f32::from(level) / 1000.0;
        let height = metrics.px(3.0 + ratio * (WAVE_HEIGHT - 3.0));
        let x = left + metrics.px(index as f32 * (BAR_WIDTH + BAR_GAP));
        let y = top + (row_height - height) / 2.0;
        let color = if level == 0 {
            theme_alpha(metrics.theme.colors.pos, 110)
        } else {
            tone_color(frame.tone, metrics.theme)
        };
        canvas.fill_round_rect(
            x,
            y,
            metrics.px(BAR_WIDTH),
            height,
            metrics.px(BAR_WIDTH / 2.0),
            color,
        );
    }
}

fn tone_color(tone: VoiceTone, theme: &Theme) -> crate::Color {
    match tone {
        VoiceTone::Listening => theme.colors.accent,
        VoiceTone::Working => theme.colors.cloud,
        VoiceTone::Success => theme.colors.caret,
        VoiceTone::Warning => theme.colors.correction,
    }
}

fn theme_alpha(mut color: crate::Color, alpha: u8) -> crate::Color {
    color.a = alpha;
    color
}
