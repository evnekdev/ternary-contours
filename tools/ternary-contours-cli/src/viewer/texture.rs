use eframe::egui;

use crate::RenderedBitmap;

pub struct RenderedTexture {
    handle: egui::TextureHandle,
    pub width: u32,
    pub height: u32,
}

impl RenderedTexture {
    pub fn from_bitmap(ctx: &egui::Context, bitmap: RenderedBitmap) -> Result<Self, String> {
        validate_bitmap(&bitmap)?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [bitmap.width as usize, bitmap.height as usize],
            &bitmap.rgba,
        );
        Ok(Self {
            handle: ctx.load_texture("liquidus-projection", image, egui::TextureOptions::LINEAR),
            width: bitmap.width,
            height: bitmap.height,
        })
    }

    pub fn update(&mut self, bitmap: RenderedBitmap) -> Result<(), String> {
        validate_bitmap(&bitmap)?;
        self.width = bitmap.width;
        self.height = bitmap.height;
        self.handle.set(
            egui::ColorImage::from_rgba_unmultiplied(
                [bitmap.width as usize, bitmap.height as usize],
                &bitmap.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        Ok(())
    }

    pub fn id(&self) -> egui::TextureId {
        self.handle.id()
    }
}

fn validate_bitmap(bitmap: &RenderedBitmap) -> Result<(), String> {
    let expected = usize::try_from(bitmap.width)
        .ok()
        .and_then(|width| {
            usize::try_from(bitmap.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "texture dimensions are too large".to_owned())?;
    if bitmap.width == 0 || bitmap.height == 0 || bitmap.rgba.len() != expected {
        return Err("renderer returned an invalid texture buffer".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_rgba_buffer() {
        assert!(
            validate_bitmap(&RenderedBitmap {
                width: 2,
                height: 2,
                rgba: vec![0; 3],
            })
            .is_err()
        );
    }
}
