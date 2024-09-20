use std::iter::Peekable;

use float_ord::FloatOrd;
use glam::{vec2, Vec2, Vec3};
use tracing::warn;

use crate::{
    format::font::{GlyphTrait, LazyFont},
    layout::parser::{LayouterParser, ParsedCommand},
    time::Ticks,
    vm::command::types::MessageTextLayout,
};

#[derive(Debug, Clone, Copy)]
pub struct LayoutedChar {
    pub time: Ticks,
    pub position: Vec2,
    pub color: Vec3,
    pub size: GlyphSize,
    pub fade: f32,
    pub codepoint: u16,
}

#[derive(Debug, Clone)]
pub enum ActionType {
    SetLipSync(bool),
    VoiceVolume(f32),
    Voice(String),
    SignalSection,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub time: Ticks,
    pub action_type: ActionType,
}

/// Represents layouter state directly settable by the user.
#[derive(Debug, Copy, Clone)]
pub struct LayouterState {
    /// Font size, in relative units (0.1 - 2.0)
    pub font_size: f32,
    pub text_color: Vec3,
    /// Text draw speed (well, actually it's time to draw one pixel)
    pub text_draw_speed: f32,
    pub fade: f32,
    /// Whether text should be displayed instantly, regardless of `text_draw_speed` and `fade`
    pub instant: bool,
}

impl Default for LayouterState {
    fn default() -> Self {
        Self {
            font_size: 1.0,
            text_color: Vec3::new(1.0, 1.0, 1.0),
            // TODO: those are not correct
            // TODO: make those into newtypes
            text_draw_speed: 0.1,
            fade: 0.01,
            instant: false,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct GlyphSize {
    pub scale: f32,
    pub horizontal_scale: f32,
    pub advance_width: f32,
    pub line_height: f32,
    pub width: f32,
    pub height: f32,
}

impl GlyphSize {
    pub fn size(&self) -> Vec2 {
        vec2(self.width, self.height)
    }

    pub fn scale_horizontal(&mut self, scale: f32) {
        self.advance_width *= scale;
        self.width *= scale;
        self.horizontal_scale *= scale;
    }
}

/// The environment for which the text should be layouted. This affects details like how the
/// character name will be positioned
#[derive(Copy, Clone)]
pub enum LayoutingMode {
    /// Text in the message box: dialogue lines and narration, including character names
    MessageText,
    /// Text in the message backlog: dialogue lines and narration, but also chapter titles, etc.
    LogText,
    /// Text outside of a dialogue environment: for example in popup boxes
    GenericText,
}

#[derive(Copy, Clone)]
pub struct LayoutParams<'a> {
    pub font: &'a LazyFont,
    pub layout_width: f32,
    pub character_name_layout_width: f32,
    pub base_font_height: f32,
    pub furigana_font_height: f32,
    pub font_horizontal_base_scale: f32,
    pub text_layout: MessageTextLayout,
    pub default_state: LayouterState,
    pub has_character_name: bool,
    pub mode: LayoutingMode,
}

impl<'a> LayoutParams<'a> {
    fn glyph_size(&self, font_size: f32, codepoint: u16) -> GlyphSize {
        let line_height = self.base_font_height * font_size;
        let scale = line_height / self.font.get_line_height() as f32;
        let horizontal_scale = scale * self.font_horizontal_base_scale;

        let glyph = self.font.get_glyph_for_character(codepoint).get_info();
        let height = glyph.actual_height as f32 * scale;
        let width = glyph.actual_width as f32 * horizontal_scale;
        let advance_width = glyph.advance_width as f32 * horizontal_scale;

        GlyphSize {
            scale,
            horizontal_scale,
            advance_width,
            line_height,
            width,
            height,
        }
    }
}

struct Layouter<'a> {
    parser: Peekable<LayouterParser<'a>>,
    params: LayoutParams<'a>,
    state: LayouterState,
    /// Layouted chars, grouped by line
    chars: Vec<Vec<LayoutedChar>>,
    pending_chars: Vec<LayoutedChar>,
    position: Vec2,
    time: Ticks,
}

impl<'a> Layouter<'a> {
    fn on_char(&mut self, c: char) {
        assert!((c as u32) < 0x10000);
        let codepoint = c as u16;

        let size = self.params.glyph_size(self.state.font_size, codepoint);
        let fade_time = if self.state.instant {
            0.0_f32
        } else {
            self.state.text_draw_speed * size.width
        };

        // TODO: handle special cases for brackets
        // TODO: handle furigana

        self.pending_chars.push(LayoutedChar {
            time: self.time,
            position: vec2(self.position.x, 0.0), // do not set y position yet, it will be set when we know which line this char is on
            color: self.state.text_color,
            size,
            fade: fade_time,
            codepoint,
        });

        self.position.x += size.advance_width;

        if !self.state.instant {
            self.time += Ticks::from_f32(self.state.text_draw_speed * size.advance_width);
        }

        // TODO: handle full stops (they add more delay)

        // TODO: where are overflows handled? On the linefeed?
    }

    fn finalize_line(&mut self, chars: &[LayoutedChar], last_line: bool, x_pos: f32) {
        // TODO: there are flags.... I think they have to do with difference between text alignment 0 & 1

        // Find the maximum height of a char in the line, or if there are no chars in the line, use the height a char
        // would have at the current font size
        let max_line_height = chars
            .iter()
            .map(|c| FloatOrd(c.size.line_height))
            .max()
            .map(|ord| ord.0)
            .unwrap_or(self.params.base_font_height * self.state.font_size);

        let furigana_height = self.params.furigana_font_height; // TODO: there is an "always leave space for furigana" flag

        // Find the total width of all chars in the line, or 0 if there are none
        let width = chars
            .iter()
            .map(|c| FloatOrd(c.position.x + c.size.advance_width))
            .max()
            .map(|ord| ord.0)
            .unwrap_or(0.0_f32)
            - x_pos;

        // let start_x = chars
        //     .iter()
        //     .map(|c| FloatOrd(c.position.x))
        //     .min()
        //     .unwrap()
        //     .0;

        // if we are not the last line, we haven't overflowed yet
        let should_stretch = !last_line
            && self.params.layout_width > width
            && self.params.text_layout == MessageTextLayout::Left
            && self.params.layout_width - width < self.params.layout_width * 0.05;

        let fit_scale = if !last_line {
            // if we are not at the last line, the line should be full
            // and usually this means that it has overflowed
            // squish text a bit to make it fit (probably more visually pleasing?)
            self.params.layout_width / width
        } else {
            1.0
        };

        let font = self.params.font;

        let line_ascent =
            (max_line_height / font.get_line_height() as f32) * font.get_ascent() as f32;

        // TODO: handle hiragana
        // TODO: handle special cases for brackets

        let x_offset = match self.params.text_layout {
            MessageTextLayout::Left => 0.0,
            MessageTextLayout::Layout1 => 0.0,
            MessageTextLayout::Center => (self.params.layout_width - width) / 2.0,
            MessageTextLayout::Right => self.params.layout_width - width,
        };

        // Append line to chars
        self.chars.push(
            chars
                .iter()
                .cloned()
                .map(|mut c| {
                    // align the text according to the layout params
                    c.position.x += x_offset;

                    // move the text to the beginning of the real line
                    // x might be larger than we want if an overflow happened
                    c.position.x -= x_pos;

                    // move the glyph on its line y coordinate (previously it was zero)
                    c.position.y += self.position.y;
                    // make sure that the glyph is on the baseline (doing it here because font size might change on the line)
                    c.position.y += line_ascent;
                    // leave space for furigana
                    // TODO: we, obviously, should not do this when there is no furigana
                    c.position.y += furigana_height;

                    // if we are overflowing - make it fit by squishing the text
                    c.position.x *= fit_scale;
                    c.size.scale_horizontal(fit_scale);

                    // if needed - make the text fit by stretching it
                    if should_stretch {
                        // I don't get this formula...
                        // also it seems to do something strange
                        // TODO: figure this stuff out
                        // c.position.x = (self.params.layout_width - c.size.width)
                        //     * (self.position.x
                        //         / (self.position.x + (width - (self.position.x + c.size.width))));
                    }
                    c
                })
                .collect(),
        );

        self.position.x = 0.0;

        self.position.y += max_line_height + furigana_height + 4.0 /* TODO: this is one of the many obscure line height-type parameters */;
    }

    fn on_newline(&mut self, wrap: bool) {
        let chars = std::mem::take(&mut self.pending_chars);

        let mut start = 0;
        let mut x_pos = 0.0;

        if wrap {
            // split into lines on overflows
            // TODO: implement word wrapping?
            for (i, c) in chars.iter().enumerate() {
                // if the start of the character is outside of the layout width
                if c.position.x - x_pos > self.params.layout_width
                    // or if the end of the character is outside of the layout width * 1.05
                    || c.position.x + c.size.width - x_pos > self.params.layout_width * 1.05
                /* allow a bit of overflow, the chars will be rescaled */
                {
                    self.finalize_line(&chars[start..i], false, x_pos);
                    x_pos = c.position.x;
                    start = i;
                }
            }
        }

        // TODO: handle overflows
        self.finalize_line(&chars[start..], true, x_pos);
        self.pending_chars.clear();
    }

    fn finalize(mut self) -> Vec<Vec<LayoutedChar>> {
        // TODO: close furigana
        self.on_newline(true);
        self.chars
    }
}

pub enum BlockExitCondition {
    /// Wait for user to press "Advance" button
    ClickWait,
    /// Wait for the VM to signal us with MSGSIGNAL command
    /// The number specified the signal number (they are counted consecutively, from 0)
    Signal(u32),
    /// Do not wait for anything to leave this block, just go to the next one (or finish the message)
    None,
}

/// One message is split into multiple blocks.
/// _Usually_ blocks are separated by click-wait commands (@k)
/// but sometimes they are separated by sync commands (@y)
///
/// Blocks are also kind of a single skip-able unit:
///  by pressing "Advance" button you skip the whole block.
///
/// For... reasons, blocks live separately from the characters,
///  their boundaries instead defined by the time they start and end.
///
/// They also contain exit conditions, which are used to determine
///  when the next block (or message) should be executed.
pub struct Block {
    pub exit_condition: BlockExitCondition,
    pub start_time: Ticks,
    pub end_time: Ticks,
}

impl Block {
    pub fn completed(&self, time: Ticks) -> bool {
        time >= self.end_time
    }
}

struct BlockBuilder {
    final_wait: bool,
    time_start: Ticks,
    signal_number: u32,
    blocks: Vec<Block>,
}

impl BlockBuilder {
    /// An artificial gap between blocks
    /// Needed because we don't want the fade-in of the next block to interfere
    const TIME_GAP: Ticks = Ticks::from_u32(1000);

    fn new() -> Self {
        Self {
            final_wait: true,
            time_start: Ticks::ZERO,
            signal_number: 0,
            blocks: Vec::new(),
        }
    }

    fn click_wait(&mut self, time: &mut Ticks) {
        self.blocks.push(Block {
            exit_condition: BlockExitCondition::ClickWait,
            start_time: self.time_start,
            end_time: *time,
        });
        *time += Self::TIME_GAP;
        self.time_start = *time;
    }

    fn sync(&mut self, time: &mut Ticks) {
        self.blocks.push(Block {
            exit_condition: BlockExitCondition::Signal(self.signal_number),
            start_time: self.time_start,
            end_time: *time,
        });
        self.signal_number += 1;
        *time += Self::TIME_GAP;
        self.time_start = *time;
    }

    fn no_final_wait(&mut self) {
        self.final_wait = false;
    }

    fn finalize(mut self, time: Ticks) -> Vec<Block> {
        self.blocks.push(Block {
            exit_condition: if self.final_wait {
                BlockExitCondition::ClickWait
            } else {
                BlockExitCondition::None
            },
            start_time: self.time_start,
            end_time: time,
        });
        self.blocks
    }
}

struct ActionsBuilder {
    actions: Vec<Action>,
}

impl ActionsBuilder {
    fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    fn action(&mut self, time: Ticks, action: ActionType) {
        self.actions.push(Action {
            time,
            action_type: action,
        });
    }

    fn finalize(self) -> Vec<Action> {
        self.actions
    }
}

pub struct LayoutedMessage {
    pub character_name_chars: Option<Vec<LayoutedChar>>,
    pub chars: Vec<LayoutedChar>,
    pub actions: Vec<Action>,
    pub blocks: Vec<Block>,
}

pub fn layout_text(params: LayoutParams, text: &str) -> LayoutedMessage {
    let mut layouter = Layouter {
        parser: LayouterParser::new(text).peekable(),
        params,
        state: params.default_state,
        chars: Vec::new(),
        pending_chars: Vec::new(),
        position: vec2(0.0, 0.0),
        time: Ticks::ZERO,
    };

    let mut block_builder = BlockBuilder::new();
    let mut actions_builder = ActionsBuilder::new();

    let layout_mode = layouter.params.mode;
    match layout_mode {
        LayoutingMode::MessageText => {
            // Character names are 0.9 font size in message boxes, and displayed instantly
            layouter.state.instant = true;
            layouter.state.font_size = 0.9;
        }
        _ => {}
    }

    // NOTE: the first line is always the character name, even if the message box does not show it
    // (it's ignored for that case)
    //
    // State changes in the character name (empirical):
    //  - font size: is ignored completely
    //  - colour: is applied for remaining chars in the character name, and for the following message text
    //  - text draw speed and fade speed: preserved for the message text but do not apply to the character name,
    //    which is always printed instantly
    let mut character_name = true; // if we are currently processing the character name (i.e. the first line)
    if layouter.parser.peek().is_some() {
        // Not using a for loop because of borrow checker
        while let Some(command) = layouter.parser.next() {
            match command {
                ParsedCommand::Char(c) => layouter.on_char(c),
                ParsedCommand::EnableLipsync => {
                    actions_builder.action(layouter.time, ActionType::SetLipSync(true))
                }
                ParsedCommand::DisableLipsync => {
                    actions_builder.action(layouter.time, ActionType::SetLipSync(false))
                }
                ParsedCommand::Furigana(_) => warn!("Furigana layout command is not implemented"),
                ParsedCommand::FuriganaStart => {
                    warn!("FuriganaStart layout command is not implemented")
                }
                ParsedCommand::FuriganaEnd => {
                    warn!("FuriganaEnd layout command is not implemented")
                }
                ParsedCommand::SetFade(fade) => layouter.state.fade = fade,
                ParsedCommand::SetColor(color) => {
                    layouter.state.text_color = color.unwrap_or(Vec3::new(1.0, 1.0, 1.0))
                }
                ParsedCommand::NoFinalClickWait => block_builder.no_final_wait(),
                ParsedCommand::ClickWait => block_builder.click_wait(&mut layouter.time),
                ParsedCommand::VoiceVolume(volume) => {
                    actions_builder.action(layouter.time, ActionType::VoiceVolume(volume))
                }
                ParsedCommand::Newline => {
                    // If character_name is true, finalise the character name part. Then set
                    // character_name to false to signify that we are now processing the main message text
                    // If it was false in the first place, just do a normal newline.
                    if character_name {
                        layouter.on_newline(false); // No line wrapping in the character name

                        // We are finishing the character name part, so reset the instant state and font size to the normal values
                        layouter.state.instant = false;
                        layouter.state.font_size = 1.0;
                        character_name = false
                    } else {
                        layouter.on_newline(true);
                    }
                }
                ParsedCommand::TextSpeed(speed) => layouter.state.text_draw_speed = speed,
                ParsedCommand::SimultaneousStart => todo!(),
                ParsedCommand::Voice(filename) => {
                    actions_builder.action(layouter.time, ActionType::Voice(filename))
                }
                ParsedCommand::Wait(time) => layouter.time += time,
                ParsedCommand::Sync => block_builder.sync(&mut layouter.time),
                ParsedCommand::FontSize(size) => {
                    // Font size changes in the character name are completely ignored
                    if !character_name {
                        layouter.state.font_size = size;
                    }
                }
                ParsedCommand::Signal => {
                    actions_builder.action(layouter.time, ActionType::SignalSection)
                }
                ParsedCommand::InstantTextStart => todo!(),
                ParsedCommand::InstantTextEnd => todo!(),
                ParsedCommand::BoldTextStart => todo!(),
                ParsedCommand::BoldTextEnd => todo!(),
            }
        }
    }

    let blocks = block_builder.finalize(layouter.time);
    let actions = actions_builder.finalize();

    let chars_by_line = layouter.finalize();

    let (character_name_chars, chars) = match layout_mode {
        // In message/log mode, the first line represents the character name (or is empty if not present).
        LayoutingMode::MessageText | LayoutingMode::LogText => {
            let mut iter = chars_by_line.into_iter();
            // Get the first line; if it is empty, convert it to None
            let character_name_chars = iter.next().filter(|v| !v.is_empty());
            let chars = iter.flatten().collect();
            (character_name_chars, chars)
        }
        // Otherwise, we just care about the main text
        LayoutingMode::GenericText => (None, chars_by_line.into_iter().flatten().collect()),
    };

    LayoutedMessage {
        character_name_chars,
        chars,
        actions,
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::BufReader};

    use super::*;

    fn is_sorted<T, F, K>(data: &[T], mut map: F) -> bool
    where
        K: Ord,
        F: FnMut(&T) -> K,
    {
        data.windows(2).all(|w| map(&w[0]) <= map(&w[1]))
    }

    fn test_layout(text: &str) -> Vec<LayoutedChar> {
        // NOTICE: here we need to use a font
        // it is an asset, so we need to load it from __somewhere__
        // having tests that depend on assets is not ideal
        // maybe I can create my own font for testing purposes?
        // use the one from assets for now
        let font = File::open("../shin/assets/data/newrodin-medium.fnt").unwrap();
        let mut font = BufReader::new(font);
        let font = rfvp_core::format::font::read_lazy_font(&mut font).unwrap();

        let params = LayoutParams {
            font: &font,
            layout_width: 1500.0,
            character_name_layout_width: 384.0,
            base_font_height: 50.0,
            furigana_font_height: 20.0,
            font_horizontal_base_scale: 0.9697,
            text_layout: MessageTextLayout::Left,
            default_state: LayouterState::default(),
            has_character_name: true,
            mode: LayoutingMode::MessageText,
        };

        let message = layout_text(params, text);

        assert!(is_sorted(&message.chars, |c| c.time));
        assert!(is_sorted(&message.actions, |a| a.time));
        assert!(is_sorted(&message.blocks, |b| b.start_time));
        assert!(message.blocks.iter().all(|b| b.end_time >= b.start_time));

        message.chars
    }

    #[test]
    fn test_simple() {
        let result = test_layout("@rHello, world!");
        println!("{:#?}", result);
    }

    #[test]
    #[ignore]
    fn test_tsu() {
        let result = test_layout(
            "@r埃と甘ったるい異臭の入り混じった薄暗い書斎に、年輩の男たちの姿はあった。",
        );

        let tsu = result[3];
        assert_eq!(tsu.codepoint, 'っ' as u16);

        const EXPECTED_ASPECT_RATIO: f32 = 104.0 / 80.0;
        let aspect_ratio = tsu.size.width / tsu.size.height;

        // divide max by min to get the ratio between aspect ratios
        let ratio =
            aspect_ratio.max(EXPECTED_ASPECT_RATIO) / aspect_ratio.min(EXPECTED_ASPECT_RATIO);
        // the ratio will always be larger than 1 and should be close to 1
        assert!(ratio < 1.09);

        // the の should still be on the first line
        let c = result[29];
        assert_eq!(c.codepoint, 'の' as u16);
        assert_eq!(c.position.y, 40.625); // TODO: why this fails?

        // while the 姿 should be on the second line
        let c = result[30];
        assert_eq!(c.codepoint, '姿' as u16);
        assert!(c.position.y > 40.625);
    }
}
