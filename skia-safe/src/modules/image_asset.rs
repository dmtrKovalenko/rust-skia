use crate::Data;
use crate::{prelude::*, Image, Matrix, SamplingOptions};
use skia_bindings::{
    self as sb, skresources_ImageAsset, skresources_ImageAsset_FrameData,
    skresources_ImageAsset_SizeFit, C_ImageFrameData_Destruct, C_ImageFrameData_Make,
    C_RustImageAsset_New, RustImageAsset, RustImageAsset_Param, SkRefCnt, SkRefCntBase,
    TraitObject,
};
use std::mem::ManuallyDrop;

pub use sb::skresources_ImageDecodeStrategy as ImageDecodeStrategy;

pub type ImageAsset = RCHandle<skresources_ImageAsset>;
require_base_type!(skresources_ImageAsset, SkRefCnt);

impl NativeRefCountedBase for skresources_ImageAsset {
    type Base = SkRefCntBase;
}

impl ImageAsset {
    pub fn is_multi_frame(&self) -> bool {
        unsafe { sb::C_ImageAsset_isMultiFrame(self.native_mut_force()) }
    }

    pub fn from_data(
        data: impl Into<Data>,
        decode_strategy: impl Into<Option<ImageDecodeStrategy>>,
    ) -> Option<Self> {
        let decode_strategy = decode_strategy
            .into()
            .unwrap_or(ImageDecodeStrategy::LazyDecode);

        ImageAsset::from_ptr(unsafe {
            sb::C_MultiFrameImageAsset_Make(data.into().into_ptr(), decode_strategy)
        })
    }

    pub fn from_custom_image_asset(
        custom_image_asset: impl Into<NativeImageAsset>,
    ) -> Option<Self> {
        ImageAsset::from_ptr(unsafe {
            std::mem::transmute::<_, *mut skresources_ImageAsset>(custom_image_asset.into())
        })
    }
}

pub type ImageSizeFit = skresources_ImageAsset_SizeFit;
pub type ImageFrameData = skresources_ImageAsset_FrameData;

/// A safe wrapper around [`ImageFrameData`] that properly releases the internal
/// `sk_sp<SkImage>` reference when dropped. This ensures correct memory management
/// when sharing images between Rust and the Skia SVG resource provider.
///
/// Created via [`ImageFrame::from_image`]. When returned from
/// [`CustomImageAsset::get_frame_data`], the wrapper is consumed and ownership of the
/// `sk_sp<SkImage>` reference transfers to C++.
pub struct ImageFrame {
    inner: ImageFrameData,
}

impl ImageFrame {
    /// Creates a new `ImageFrame` from an [`Image`] reference.
    ///
    /// The image's reference count is incremented (via clone), so the caller retains
    /// their own reference while the `ImageFrame` holds an independent one. This makes
    /// it safe to share the same `Image` across multiple frames and with the resource
    /// provider.
    pub fn from_image(
        image: &Image,
        matrix: Matrix,
        sampling: SamplingOptions,
        scaling: ImageSizeFit,
    ) -> Self {
        let inner = unsafe {
            C_ImageFrameData_Make(
                image.clone().into_ptr(),
                matrix.into_native(),
                sampling.into_native(),
                scaling,
            )
        };
        Self { inner }
    }
}

impl Drop for ImageFrame {
    fn drop(&mut self) {
        unsafe {
            C_ImageFrameData_Destruct(&mut self.inner);
        }
    }
}

pub trait CustomImageAsset {
    fn is_multi_frame(&self) -> bool;
    fn get_frame_data(&self, t: f32) -> ImageFrame;
}

pub type NativeImageAsset = RCHandle<RustImageAsset>;

impl<T: CustomImageAsset + 'static> From<T> for NativeImageAsset {
    fn from(value: T) -> Self {
        let b: Box<dyn CustomImageAsset> = Box::new(value);
        Self::from(b)
    }
}

impl NativeRefCountedBase for RustImageAsset {
    type Base = SkRefCntBase;
}

impl From<Box<dyn CustomImageAsset>> for NativeImageAsset {
    fn from(image_asset: Box<dyn CustomImageAsset>) -> Self {
        let param = RustImageAsset_Param {
            trait_: unsafe {
                std::mem::transmute::<Box<dyn CustomImageAsset>, TraitObject>(image_asset)
            },
            drop: Some(drop),
            isMultiFrame: Some(is_multi_frame),
            getFrameData: Some(get_frame_data),
        };

        let skia_image_asset =
            NativeImageAsset::from_ptr(unsafe { C_RustImageAsset_New(&param) }).unwrap();

        return skia_image_asset;

        extern "C" fn drop(asset: TraitObject) {
            std::mem::drop(unsafe {
                std::mem::transmute::<TraitObject, Box<dyn CustomImageAsset>>(asset)
            });
        }

        extern "C" fn is_multi_frame(asset: TraitObject) -> bool {
            unsafe { asset_ref(&asset).is_multi_frame() }
        }

        extern "C" fn get_frame_data(asset: TraitObject, t: f32) -> ImageFrameData {
            let frame = ManuallyDrop::new(unsafe { asset_ref(&asset).get_frame_data(t) });
            unsafe { std::ptr::read(&frame.inner) }
        }

        unsafe fn asset_ref(asset: &TraitObject) -> &dyn CustomImageAsset {
            std::mem::transmute(*asset)
        }
    }
}
