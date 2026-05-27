#[path = "utils/stable_hash.rs"]
pub mod stable_hash;

pub mod ani {
    use alloc::vec::Vec;

    #[derive(Clone, Debug)]
    pub struct CustomCursor;

    #[derive(Clone, Debug, Default)]
    pub struct CursorBundle {
        pub frames: Vec<CustomCursor>,
        pub current_frame: usize,
    }

    impl CursorBundle {
        pub fn reset(&mut self) {
            self.current_frame = 0;
        }

        pub fn update(&mut self) -> CustomCursor {
            if self.frames.is_empty() {
                return CustomCursor;
            }
            let frame = self.frames[self.current_frame % self.frames.len()].clone();
            self.current_frame = self.current_frame.wrapping_add(1);
            frame
        }
    }
}

pub mod file {
    use crate::path::PathBuf;
    use alloc::string::{String, ToString};

    #[derive(Clone, Debug, Default)]
    pub struct PathBuilder {
        path: String,
    }

    impl PathBuilder {
        pub fn new() -> Self {
            Self {
                path: String::new(),
            }
        }

        pub fn append(mut self, value: &str) -> Self {
            if !self.path.is_empty() && !self.path.ends_with('/') {
                self.path.push('/');
            }
            self.path.push_str(value.trim_matches('/'));
            self
        }

        pub fn get(self) -> String {
            self.path
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct BasePath {
        path: String,
    }

    impl BasePath {
        pub fn join(&self, path: &str) -> PathBuf {
            let mut out = PathBuf::from(self.path.as_str());
            out.push(path);
            out
        }

        pub fn get_path(&self) -> PathBuf {
            PathBuf::from(self.path.as_str())
        }

        pub fn get_path_string(&self) -> String {
            self.path.clone()
        }
    }

    pub fn app_base_path() -> BasePath {
        BasePath::default()
    }

    pub fn hcb_root_path() -> BasePath {
        BasePath::default()
    }
}

pub mod maths {
    include!("utils/maths.rs");
}

pub mod time {
    include!("utils/time.rs");
}
