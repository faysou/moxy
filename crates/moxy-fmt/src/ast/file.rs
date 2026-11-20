use moxy_ast::File;

use crate::{FmtError, Format, Formatter};

impl Format for File {
    fn format(&self, f: &mut Formatter) -> Result<(), FmtError> {
        if let Some(shebang) = &self.shebang {
            f.text(shebang)?;

            if !self.attrs.is_empty() || !self.items.is_empty() {
                f.hard_break()?;
            }
        }

        self.attrs.format(f)?;
        let mut first = true;

        for item in &self.items {
            if !first {
                f.hard_break()?;
                f.hard_break()?;
            }

            item.format(f)?;
            first = false;
        }

        Ok(())
    }
}
