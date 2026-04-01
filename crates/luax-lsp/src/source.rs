use tower_lsp::lsp_types::{Position, Range};

#[derive(Debug, Clone)]
pub struct SourceMap {
    text: String,
    line_starts: Vec<usize>,
}

impl SourceMap {
    pub fn new(text: String) -> Self {
        let mut line_starts = vec![0];
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { text, line_starts }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn offset_to_position(&self, offset: usize) -> Position {
        let clamped = offset.min(self.text.len());
        let line_idx = match self.line_starts.binary_search(&clamped) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx];
        let line_slice = &self.text[line_start..clamped];
        let character = line_slice.encode_utf16().count() as u32;
        Position::new(line_idx as u32, character)
    }

    pub fn position_to_offset(&self, position: Position) -> usize {
        let line = position.line as usize;
        if line >= self.line_starts.len() {
            return self.text.len();
        }
        let line_start = self.line_starts[line];
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        let line_slice = &self.text[line_start..line_end];
        let mut utf16_count = 0usize;
        for (idx, ch) in line_slice.char_indices() {
            if utf16_count >= position.character as usize {
                return line_start + idx;
            }
            utf16_count += ch.len_utf16();
        }
        line_end
    }

    pub fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.offset_to_position(start), self.offset_to_position(end))
    }

    pub fn slice(&self, start: usize, end: usize) -> &str {
        &self.text[start.min(self.text.len())..end.min(self.text.len())]
    }

    pub fn offset_of_prefix_before(&self, position: Position) -> usize {
        self.position_to_offset(position)
    }
}
