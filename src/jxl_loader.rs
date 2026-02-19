
// from https://github.com/emilk/egui/blob/main/crates/egui_extras/src/loaders/webp_loader.rs
// with webp replaced with jxl

use ahash::HashMap;
use egui::{
    ColorImage, FrameDurations, Id, decode_animated_image_uri,
    load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint},
    mutex::Mutex,
};
use std::{io::Cursor, mem::size_of, sync::Arc, time::Duration};

#[derive(Clone)]
enum Jxl {
    Static(Arc<ColorImage>),
    Animated(AnimatedImage),
}

struct JxlDecoder;

use crate::dec::*;
use jxl::api::*;

impl JxlDecoder{
	fn new(mut data: &[u8]) -> JxlDecoder{
		if let Ok(x) = decode_frames(
			&mut data,
			JxlDecoderOptions::default(),
			None,
			None,
			&[
				OutputDataType::U8,
				OutputDataType::U16,
				OutputDataType::F16,
				OutputDataType::F32,
			],
			true,
			false,
			None,
			false,
		){
			let (image_data, dur): (DecodeOutput, Duration) = x;
			let (width, height) = image_data.size;
		}
		JxlDecoder
	}
	fn has_animation(&self) -> bool {true}
	fn color_type(&self) -> bool {true}
	fn into_frames(&self) -> Vec<Result<u16, &str>> {vec![]}
	fn dimensions(&self) -> (bool, bool) {todo!()}
	fn total_bytes(&self) -> bool {true}
	fn read_image(&self, a: &mut Vec<u8>) -> Result<bool, &str> {Ok(true)}
}

impl Jxl {
    fn load(mut data: &[u8]) -> Result<Self, String> {
        let mut decoder = JxlDecoder;
		let x = decode_frames(
			&mut data,
			JxlDecoderOptions::default(),
			None,
			None,
			&[
				OutputDataType::U8,
				OutputDataType::U16,
				OutputDataType::F16,
				OutputDataType::F32,
			],
			true,
			false,
			None,
			false,
		).unwrap();
		let (image_data, dur): (DecodeOutput, Duration) = x;
		let (width, height) = image_data.size;

        if decoder.has_animation() {
			let mut images = vec![];
            let mut durations = vec![];
			for frame in &image_data.frames {
				{
					const NANOS_PER_MILLI: u32 = 1_000_000;
					const NANOS_PER_MICRO: f64 = 1_000.0;
					const MILLIS_PER_SEC: f64 = 1_000.0;
					let millis = frame.duration;
					let secs = (millis / MILLIS_PER_SEC) as u64;
					let subsec_millis = (millis % MILLIS_PER_SEC) as u32;
					// SAFETY: (x % 1_000) * 1_000_000 < 1_000_000_000
					//         => x % 1_000 < 1_000
					let subsec_nanos = subsec_millis * NANOS_PER_MILLI;
					let delay: Duration = Duration::new(secs, subsec_nanos);
					durations.push(delay);
				}
				let size: [usize; 2] = [width, height];
				let mut pixels: Vec<u8> = vec![];
				let chan = &frame.channels[0];
				for y in 0..height {
					pixels.extend(chan.row(y));
				}
				images.push(Arc::new(ColorImage::from_rgba_unmultiplied(
                    size,
                    pixels.as_slice(),
                )));
			}
            Ok(Self::Animated(AnimatedImage {
                frames: images,
                frame_durations: FrameDurations::new(durations),
            }))
        } else {
            // color_type() of JxlDecoder only returns Rgb8/Rgba8 variants of ColorType
            let create_image = match decoder.color_type() {
                true => ColorImage::from_rgb,
                false => ColorImage::from_rgba_unmultiplied
            };

            let (width, height) = decoder.dimensions();
            let size = decoder.total_bytes() as usize;

            let mut data = vec![0; size];
            decoder
                .read_image(&mut data)
                .map_err(|error| format!("Jxl image read failure ({error})"))?;

            Ok(Self::Static(Arc::new(create_image(
                [width as usize, height as usize],
                &data,
            ))))
        }
    }

    fn get_image(&self, frame_index: usize) -> Arc<ColorImage> {
        match self {
            Self::Static(image) => Arc::clone(image),
            Self::Animated(animation) => animation.get_image_by_index(frame_index),
        }
    }

    pub fn byte_len(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Static(image) => image.pixels.len() * size_of::<egui::Color32>(),
                Self::Animated(animation) => animation.byte_len(),
            }
    }
}

#[derive(Debug, Clone)]
pub struct AnimatedImage {
    frames: Vec<Arc<ColorImage>>,
    frame_durations: FrameDurations,
}

impl AnimatedImage {
    pub fn byte_len(&self) -> usize {
        size_of::<Self>()
            + self
                .frames
                .iter()
                .map(|image| {
                    image.pixels.len() * size_of::<egui::Color32>() + size_of::<Duration>()
                })
                .sum::<usize>()
    }

    pub fn get_image_by_index(&self, index: usize) -> Arc<ColorImage> {
        Arc::clone(&self.frames[index % self.frames.len()])
    }
}

type Entry = Result<Jxl, String>;

#[derive(Default)]
pub struct JxlLoader {
    cache: Mutex<HashMap<String, Entry>>,
}

impl JxlLoader {
    pub const ID: &'static str = egui::generate_loader_id!(JxlLoader);
}

impl ImageLoader for JxlLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(&self, ctx: &egui::Context, frame_uri: &str, _: SizeHint) -> ImageLoadResult {
        let (image_uri, frame_index) =
            decode_animated_image_uri(frame_uri).map_err(|_error| LoadError::NotSupported)?;
		log::warn!("do we accept {image_uri:?}");

        let mut cache = self.cache.lock();
        if let Some(entry) = cache.get(image_uri).cloned() {
            match entry {
                Ok(image) => Ok(ImagePoll::Ready {
                    image: image.get_image(frame_index),
                }),
                Err(error) => Err(LoadError::Loading(error)),
            }
        } else {
            match ctx.try_load_bytes(image_uri) {
                Ok(BytesPoll::Ready { bytes, .. }) => {
                    if !image_uri.ends_with(".jxl") {
                        return Err(LoadError::NotSupported);
                    }

                    log::trace!("started loading {image_uri:?}");

                    let result = Jxl::load(&bytes);

                    if let Ok(Jxl::Animated(animated_image)) = &result {
                        ctx.data_mut(|data| {
                            *data.get_temp_mut_or_default(Id::new(image_uri)) =
                                animated_image.frame_durations.clone();
                        });
                    }

                    log::trace!("finished loading {image_uri:?}");

                    cache.insert(image_uri.into(), result.clone());

                    match result {
                        Ok(image) => Ok(ImagePoll::Ready {
                            image: image.get_image(frame_index),
                        }),
                        Err(error) => Err(LoadError::Loading(error)),
                    }
                }
                Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
                Err(error) => Err(error),
            }
        }
    }

    fn forget(&self, uri: &str) {
        let _ = self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values()
            .map(|entry| match entry {
                Ok(entry_value) => entry_value.byte_len(),
                Err(error) => error.len(),
            })
            .sum()
    }
}
