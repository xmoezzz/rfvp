use std::fmt::Write;

use itertools::Itertools;
use tracing::debug;

use super::prelude::*;

fn format(format: &str, arguments: &[i32]) -> String {
    let mut result = String::new();
    let mut arguments = arguments.iter();

    let mut iter = format.split(|c| c == '%');
    if let Some(begin) = iter.next() {
        result.push_str(begin);
    }
    let mut last_was_percent = false;
    for (sub, next) in iter
        .map(Some)
        .chain(core::iter::once(None))
        .tuple_windows::<(_, _)>()
    {
        let sub = match sub {
            Some(sub) => sub,
            None => break,
        };
        if last_was_percent {
            result.push_str(sub);
            last_was_percent = false;
            continue;
        }
        // let (flags, sub) = parse_flags(sub);
        // let (width, sub) = parse_width(sub, &mut args);
        // let (precision, sub) = parse_precision(sub, &mut args);
        // let (length, sub) = parse_length(sub);

        let mut sub = sub.chars();

        let ch = sub.next().unwrap_or_else(|| {
            if next.is_some() {
                '%'
            } else {
                panic!("Ill-formatted format string")
            }
        });

        let sub = sub.as_str();
        match ch {
            '%' => {
                last_was_percent = true;
                result.push('%');
            }
            'd' | 'i' => write!(
                result,
                "{}",
                arguments.next().expect("Missing a format string argument")
            )
            .unwrap(),
            s => panic!("Unknown specifier: {}", s),
        };
        result.push_str(sub);
    }

    result
}

impl StartableCommand for command::runtime::DEBUGOUT {
    fn apply_state(&self, _state: &mut VmState) {}

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        let result = format(&self.format, &self.args);

        debug!("DEBUGOUT: {}", result);
        self.token.finish().into()
    }
}
