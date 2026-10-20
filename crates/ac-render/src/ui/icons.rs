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

#[derive(Debug, Default)]
struct Store {
    next: u32,
    images: HashMap<u32, DecodedTexture>,
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
    pub fn take(&self, id: u32) -> Option<DecodedTexture> {
        self.0.borrow_mut().images.remove(&id)
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
        let mut store = self.0.0.borrow_mut();
        store.next += 1;
        let id = store.next;
        store.images.insert(
            id,
            DecodedTexture {
                width: image.width,
                height: image.height,
                rgba: image.pixels.clone(),
            },
        );
        TextureRef::Runtime(id)
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
        let pixels = images.take(id).unwrap();
        assert_eq!((pixels.width, pixels.height, pixels.rgba.len()), (2, 1, 8));
        assert!(images.take(id).is_none());
        assert_ne!(up.upload(&Rgba::new(1, 1, vec![0; 4])), t);
    }
}
