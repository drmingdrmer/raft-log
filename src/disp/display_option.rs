use std::fmt;

pub(crate) struct DebugOption<'a, T>(pub(crate) Option<&'a T>);

impl<T> fmt::Debug for DebugOption<'_, T>
where T: fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(v) => write!(f, "{:?}", v),
            None => write!(f, "None"),
        }
    }
}
