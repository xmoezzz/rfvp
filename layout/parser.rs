use glam::Vec3;

use crate::time::Ticks;

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommand {
    /// Just your regular character (or a @U command)
    Char(char),
    /// @+
    EnableLipsync,
    /// @-
    DisableLipsync,
    /// @b
    Furigana(String),
    /// @<
    FuriganaStart,
    /// @>
    FuriganaEnd,
    /// @a
    SetFade(f32),
    /// @c
    SetColor(Option<Vec3>),
    /// @e
    NoFinalClickWait,
    /// @k
    ClickWait,
    /// @o
    VoiceVolume(f32),
    /// @r
    Newline,
    /// @s
    TextSpeed(f32),
    /// @t
    SimultaneousStart,
    /// @v
    Voice(String),
    /// @w
    Wait(Ticks),
    /// @y
    Sync,
    /// @z
    FontSize(f32),
    /// @|
    Signal,
    /// @[
    InstantTextStart,
    /// @]
    InstantTextEnd,
    /// @{
    BoldTextStart,
    /// @}
    BoldTextEnd,
}

pub struct LayouterParser<'a> {
    message: &'a str,
}

impl<'a> LayouterParser<'a> {
    pub fn new(message: &'a str) -> Self {
        Self { message }
    }

    fn read_argument(&mut self) -> &'a str {
        let end = self
            .message
            .find('.')
            .expect("Could not find the end of the argument");
        let argument = &self.message[..end];
        self.message = &self.message[end + 1..];
        argument
    }

    fn read_float_argument(&mut self, min: u32, max: u32, scale: f32) -> f32 {
        let argument = self.read_argument();
        let value = argument.parse::<u32>().expect("Could not parse argument");
        let value = value.clamp(min.min(max), max.max(min));
        // if min max are backwards - reverse the value
        let value = if min > max { max - value } else { value };
        value as f32 / scale
    }

    fn read_color_argument(&mut self) -> Option<Vec3> {
        let argument = self.read_argument();
        if argument.is_empty() {
            None
        } else {
            let mut chars = argument.chars();
            let r = chars.next().unwrap().to_digit(10).unwrap() as f32 / 9.0;
            let g = chars.next().unwrap().to_digit(10).unwrap() as f32 / 9.0;
            let b = chars.next().unwrap().to_digit(10).unwrap() as f32 / 9.0;
            assert!(chars.next().is_none());
            Some(Vec3::new(r, g, b))
        }
    }
}

impl Iterator for LayouterParser<'_> {
    type Item = ParsedCommand;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: make this parsing fallible

        if self.message.is_empty() {
            return None;
        }

        let mut chars = self.message.chars();
        let first_char = chars.next().unwrap();

        if first_char != '@' {
            self.message = chars.as_str();
            return Some(ParsedCommand::Char(first_char));
        }

        let second_char = chars.next().unwrap();
        self.message = chars.as_str();

        Some(match second_char {
            '+' => ParsedCommand::EnableLipsync,
            '-' => ParsedCommand::DisableLipsync,
            'b' => ParsedCommand::Furigana(self.read_argument().to_owned()),
            '<' => ParsedCommand::FuriganaStart,
            '>' => ParsedCommand::FuriganaEnd,
            'a' => ParsedCommand::SetFade(self.read_float_argument(0, u32::MAX, 1000.0)),
            'c' => ParsedCommand::SetColor(self.read_color_argument()),
            'e' => ParsedCommand::NoFinalClickWait,
            'k' => ParsedCommand::ClickWait,
            'o' => ParsedCommand::VoiceVolume(self.read_float_argument(0, 100, 100.0)),
            'r' => ParsedCommand::Newline,
            's' => ParsedCommand::TextSpeed(self.read_float_argument(100, 0, 40000.0)),
            't' => ParsedCommand::SimultaneousStart,
            'v' => ParsedCommand::Voice(self.read_argument().to_owned()),
            'w' => ParsedCommand::Wait(Ticks::from_f32(self.read_float_argument(
                0,
                u32::MAX,
                1000.0,
            ))),
            'y' => ParsedCommand::Sync,
            'z' => ParsedCommand::FontSize(self.read_float_argument(10, 200, 100.0)),
            '|' => ParsedCommand::Signal,
            '[' => ParsedCommand::InstantTextStart,
            ']' => ParsedCommand::InstantTextEnd,
            '{' => ParsedCommand::BoldTextStart,
            '}' => ParsedCommand::BoldTextEnd,
            'U' => todo!("@U layouter command parsing"),
            _ => panic!("Unknown layouter command: {}", second_char),
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::vec3;

    use super::*;

    fn parse(message: &str) -> Vec<ParsedCommand> {
        LayouterParser::new(message).collect()
    }

    #[test]
    fn test_hello() {
        let message = "Hello";
        let commands = parse(message);

        assert_eq!(
            commands,
            vec![
                ParsedCommand::Char('H'),
                ParsedCommand::Char('e'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('o')
            ]
        );
    }

    #[test]
    fn test_furigana() {
        let message = "@bかな.@<漢字@>";
        let commands = parse(message);

        assert_eq!(
            commands,
            vec![
                ParsedCommand::Furigana("かな".to_owned()),
                ParsedCommand::FuriganaStart,
                ParsedCommand::Char('漢'),
                ParsedCommand::Char('字'),
                ParsedCommand::FuriganaEnd,
            ]
        );
    }

    #[test]
    fn test_color() {
        let message = "@c940.@rHello@c.";
        let commands = parse(message);

        assert_eq!(
            commands,
            vec![
                ParsedCommand::SetColor(Some(vec3(1.0, 4.0 / 9.0, 0.0))),
                ParsedCommand::Newline,
                ParsedCommand::Char('H'),
                ParsedCommand::Char('e'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('o'),
                ParsedCommand::SetColor(None),
            ]
        );
    }

    #[test]
    fn test_wait() {
        let message = "Hello@w400.@rWorld";
        let commands = parse(message);

        assert_eq!(
            commands,
            vec![
                ParsedCommand::Char('H'),
                ParsedCommand::Char('e'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('o'),
                ParsedCommand::Wait(Ticks::from_f32(0.4)),
                ParsedCommand::Newline,
                ParsedCommand::Char('W'),
                ParsedCommand::Char('o'),
                ParsedCommand::Char('r'),
                ParsedCommand::Char('l'),
                ParsedCommand::Char('d'),
            ]
        );
    }

    #[test]
    fn test_real1() {
        let message = "@r@v00/awase6042_o.@|@y｢｢@c900.@[謹啓､謹ﾝで申ｼ上げﾙ｡@k@v00/awase6043_o.どﾁﾗﾓ破ﾗﾚﾃｲﾅｲﾓﾉﾄ知ﾘ給ｴ@]@c.｣｣";
        let commands = parse(message);

        assert_eq!(
            commands,
            vec![
                ParsedCommand::Newline,
                ParsedCommand::Voice("00/awase6042_o".to_owned()),
                ParsedCommand::Signal,
                ParsedCommand::Sync,
                ParsedCommand::Char('｢'),
                ParsedCommand::Char('｢'),
                ParsedCommand::SetColor(Some(vec3(1.0, 0.0, 0.0))),
                ParsedCommand::InstantTextStart,
                ParsedCommand::Char('謹'),
                ParsedCommand::Char('啓'),
                ParsedCommand::Char('､'),
                ParsedCommand::Char('謹'),
                ParsedCommand::Char('ﾝ'),
                ParsedCommand::Char('で'),
                ParsedCommand::Char('申'),
                ParsedCommand::Char('ｼ'),
                ParsedCommand::Char('上'),
                ParsedCommand::Char('げ'),
                ParsedCommand::Char('ﾙ'),
                ParsedCommand::Char('｡'),
                ParsedCommand::ClickWait,
                ParsedCommand::Voice("00/awase6043_o".to_owned()),
                ParsedCommand::Char('ど'),
                ParsedCommand::Char('ﾁ'),
                ParsedCommand::Char('ﾗ'),
                ParsedCommand::Char('ﾓ'),
                ParsedCommand::Char('破'),
                ParsedCommand::Char('ﾗ'),
                ParsedCommand::Char('ﾚ'),
                ParsedCommand::Char('ﾃ'),
                ParsedCommand::Char('ｲ'),
                ParsedCommand::Char('ﾅ'),
                ParsedCommand::Char('ｲ'),
                ParsedCommand::Char('ﾓ'),
                ParsedCommand::Char('ﾉ'),
                ParsedCommand::Char('ﾄ'),
                ParsedCommand::Char('知'),
                ParsedCommand::Char('ﾘ'),
                ParsedCommand::Char('給'),
                ParsedCommand::Char('ｴ'),
                ParsedCommand::InstantTextEnd,
                ParsedCommand::SetColor(None),
                ParsedCommand::Char('｣'),
                ParsedCommand::Char('｣'),
            ]
        );
    }
}
