//! Composed icons on their way to the GPU: the icon composer of `ac-ui`
//! hands each image it makes to an [`IconUploader`], which keeps it under
//! a runtime texture name until the UI renderer's texture cache asks for
//! it ([`RuntimeImages::take`]). The cache keeps the GPU copy for as long
//! as the window lives, so each image is handed over once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ac_dat::texture::DecodedTexture;
use ac_ui::draw::TextureRef;
use ac_ui::icons::{IconUploader, Rgba};

use super::gpu::UiPixels;

#[derive(Debug, Default)]
struct Store {
    next: u32,
    images: HashMap<u32, UiPixels>,
}

impl Store {
    fn insert(&mut self, pixels: UiPixels) -> TextureRef {
        self.next += 1;
        self.images.insert(self.next, pixels);
        TextureRef::Runtime(self.next)
    }
}

/// The images uploaded and not yet taken by the renderer. Clones share
/// one store (the composer's uploader and the window's texture source).
#[derive(Debug, Clone, Default)]
pub struct RuntimeImages(Rc<RefCell<Store>>);

impl RuntimeImages {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The uploader that fills this store.
    #[must_use]
    pub fn uploader(&self) -> Box<dyn IconUploader> {
        Box::new(Uploader(self.clone()))
    }

    /// The pixels of runtime texture `id`, handed over once.
    #[must_use]
    pub fn take(&self, id: u32) -> Option<UiPixels> {
        self.0.borrow_mut().images.remove(&id)
    }

    /// Keep a one-byte coverage image (drawn white with that alpha) under
    /// a new runtime texture name.
    pub fn add_coverage(&self, width: u32, height: u32, alpha: Vec<u8>) -> TextureRef {
        self.0.borrow_mut().insert(UiPixels::Coverage {
            width,
            height,
            alpha,
        })
    }

    /// Images waiting for the renderer.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.0.borrow().images.len()
    }
}

struct Uploader(RuntimeImages);

impl IconUploader for Uploader {
    fn upload(&mut self, image: &Rgba) -> TextureRef {
        self.0.0.borrow_mut().insert(UiPixels::Rgba(DecodedTexture {
            width: image.width,
            height: image.height,
            rgba: image.pixels.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_uploaded_image_is_named_and_handed_over_once() {
        let images = RuntimeImages::new();
        let mut up = images.uploader();
        let t = up.upload(&Rgba::new(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]));
        let TextureRef::Runtime(id) = t else {
            panic!("a runtime texture");
        };
        assert_eq!(images.waiting(), 1);
        let Some(UiPixels::Rgba(pixels)) = images.take(id) else {
            panic!("RGBA pixels");
        };
        assert_eq!((pixels.width, pixels.height, pixels.rgba.len()), (2, 1, 8));
        assert!(images.take(id).is_none());
        assert_ne!(up.upload(&Rgba::new(1, 1, vec![0; 4])), t);
    }

    #[test]
    fn a_coverage_image_is_kept_one_byte_a_texel_under_a_new_name() {
        let images = RuntimeImages::new();
        let rgba = images.uploader().upload(&Rgba::new(1, 1, vec![0; 4]));
        let coverage = images.add_coverage(2, 1, vec![7, 9]);
        assert_ne!(coverage, rgba);
        let TextureRef::Runtime(id) = coverage else {
            panic!("a runtime texture");
        };
        assert_eq!(
            images.take(id),
            Some(UiPixels::Coverage {
                width: 2,
                height: 1,
                alpha: vec![7, 9]
            })
        );
    }
}
