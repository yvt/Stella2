use array_intrusive_list::{Link, ListHead};
use leakypool::LeakyPool;
use quick_error::quick_error;
use std::{cell::RefCell, fmt, sync::Arc};
use tcw3_pal::{self as pal, iface::Wm as _, Bitmap, MtLock, MtSticky, Wm};

/// A bitmap created by rasterizing [`Img`]. The second value represents the
/// actual DPI scale value of the bitmap, which may or may not match the
/// `dpi_scale` passed to `Img::new_bmp`.
pub type Bmp = (Bitmap, f32);

/// An implementation of an image with an abstract representation.
pub trait Img: Send + Sync + 'static {
    /// Construct a `Bitmap` for the specified DPI scale.
    ///
    /// Returns a constructed `Bitmap` and the actual DPI scale of the `Bitmap`.
    fn new_bmp(&self, dpi_scale: f32) -> Bmp;
}

/// Represents an image with an abstract representation.
///
/// # Needs a main thread
///
/// Although this type is thread-safe, it implicitly relies on the existence
/// of a main thread (the one having access to `Wm`) for synchronization.
/// Dropping `HImg` before defining a main thread might cause a panic on some
/// backends. Therefore, you call a method such as `Wm::try_global` to ensure
/// a main thread is defined.
#[derive(Debug, Clone)]
pub struct HImg {
    inner: Arc<ImgInner<dyn Img>>,
}

struct ImgInner<T: ?Sized> {
    cache_ref: MtSticky<RefCell<ImgCacheRef>>,
    img: T,
}

#[derive(Debug)]
struct ImgCacheRef {
    /// A pointer to a `CacheImg` in `Cache::imgs`.
    img_ptr: Option<ImgPtr>,
}

impl HImg {
    pub fn new(img: impl Img) -> Self {
        Self {
            inner: Arc::new(ImgInner {
                cache_ref: MtSticky::new(RefCell::new(ImgCacheRef { img_ptr: None })),
                img,
            }),
        }
    }

    /// Construct a `Bitmap` for the specified DPI scale. Uses a global cache,
    /// which is owned by the main thread (hence the `Wm` parameter).
    ///
    /// The cache only stores `Bmp`s created for DPI scale values used by any of
    /// open windows. For other DPI scale values, this method behaves like
    /// `new_bmp_uncached`.
    ///
    /// Returns a constructed `Bitmap` and the actual DPI scale of the `Bitmap`.
    pub fn new_bmp(&self, wm: Wm, dpi_scale: f32) -> Bmp {
        let mut cache_ref = self
            .inner
            .cache_ref
            .get_with_wm(wm)
            .try_borrow_mut()
            .expect("can't call `new_bmp` recursively on the same image");

        let dpi_scale = DpiScale::new(dpi_scale).unwrap();

        let mut cache = CACHE.get_with_wm(wm).borrow_mut();

        let img_ptr = *cache_ref.img_ptr.get_or_insert_with(|| {
            // `CacheImg` isn't in the cache, create one
            cache.img_add()
        });

        // Try the cache
        if let Some(bmp) = cache.img_find_bmp(img_ptr, dpi_scale) {
            return bmp.clone();
        }

        // Not in the cache. Create a brand new `Bmp`.
        //
        // Unborrow the cache temporarily so that `Img::new_bmp` can
        // recursively call `new_bmp` for other images.
        drop(cache);

        let bmp = self.inner.img.new_bmp(dpi_scale.value());

        // Find the `CacheDpiScale` object.
        let mut cache = CACHE.get_with_wm(wm).borrow_mut();
        let dpi_scale_ptr = if let Some(x) = cache.dpi_scale_find(dpi_scale) {
            x
        } else {
            // Unrecognized DPI scale, cache is unavailable
            return bmp;
        };

        // Insert the `Bmp` to the cache.
        cache.img_add_bmp(img_ptr, dpi_scale_ptr, bmp.clone());

        bmp
    }

    /// Construct a `Bitmap` for the specified DPI scale. Does not use a cache
    /// and always calls [`Img::new_bmp`] directly.
    ///
    /// Returns a constructed `Bitmap` and the actual DPI scale of the `Bitmap`.
    pub fn new_bmp_uncached(&self, dpi_scale: f32) -> Bmp {
        self.inner.img.new_bmp(dpi_scale)
    }
}

impl Drop for ImgCacheRef {
    fn drop(&mut self) {
        if let Some(img_ptr) = self.img_ptr {
            // `ImgCacheRef` is wrapped by `MtSticky`, so `Wm::global()` will succeed
            let wm = Wm::global();
            CACHE.get_with_wm(wm).borrow_mut().img_remove(img_ptr);
        }
    }
}

impl<T: ?Sized> fmt::Debug for ImgInner<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ImgInner")
            .field("cache_ref", &self.cache_ref)
            .field("img", &((&self.img) as *const _))
            .finish()
    }
}

/// Increment the use count of the specified DPI scale value.
///
/// `HImg` caches generated bitmaps for known DPI values. The bitmaps are
/// released when the originating `Himg` is dropped or the target DPI scale is
/// no longer used.
///
/// [`dpi_scale_release`] decrements the use count.
pub fn dpi_scale_add_ref(wm: pal::Wm, dpi_scale: f32) {
    CACHE
        .get_with_wm(wm)
        .borrow_mut()
        .dpi_scale_add_ref(DpiScale::new(dpi_scale).unwrap());
}

/// Decrement the use count of the specified DPI scale value.
///
/// See [`dpi_scale_add_ref`] for more.
pub fn dpi_scale_release(wm: pal::Wm, dpi_scale: f32) {
    CACHE
        .get_with_wm(wm)
        .borrow_mut()
        .dpi_scale_release(DpiScale::new(dpi_scale).unwrap());
}

static CACHE: MtLock<RefCell<Cache>> = MtLock::new(RefCell::new(unsafe { Cache::new() }));

//
//  Cache -------+-----------------,
//               |                 |
//               v                 v
//         CacheDpiScale     CacheDpiScale
//               |                 | (bmps)
//               |        ,------, |     (bmps/img)
//      ,------, | ,------|------|-|-->,-----> CacheImg
//      |      v v |      |      v v   v
//      | ,-> CacheBmp <--|---> CacheBmp <-, (link_img)
//      | '---------------|----------------'
//      |        ^        |        ^
//      |        | ,------|--------|-->,-----> CacheImg
//      |        v |      |        v   v
//      | ,-> CacheBmp <--|---> CacheBmp <-,
//      | '------^--------|--------^-------'
//      |        |        |        |
//      '--------'        '--------' (link_dpi_scale)
//
// Assumptions:
//
//  - The number of the elements is very small - it's usually 1 or 2 and bounded
//    by the number of computer monitors connected to the user's machine.
//
#[derive(Debug)]
struct Cache {
    imgs: LeakyPool<CacheImg>,
    bmps: LeakyPool<CacheBmp>,
    // Mappings from `DpiScale` to `CacheDpiScale`. Hashtables would be overkill
    // for such a small number of elements.
    dpi_scales: Vec<CacheDpiScale>,
}

type ImgPtr = leakypool::PoolPtr<CacheImg>;
type BmpPtr = leakypool::PoolPtr<CacheBmp>;

/// A known DPI scale.
#[derive(Debug)]
struct CacheDpiScale {
    dpi_scale: DpiScale,
    /// The number of the clients that may request rasterization for this DPI
    /// scale value. When this hits zero, all cached bitmaps in `bmps` are
    /// destroyed.
    ref_count: usize,
    /// A linked-list of `CacheBmp` having this DPI scale.
    /// Elements are linked by `CacheBmp::link_dpi_scale`.
    bmps: ListHead<BmpPtr>,
}

/// An index into `Cache::dpi_scales`. Invalidated whenever the list is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheDpiScalePtr(usize);

#[derive(Debug)]
struct CacheImg {
    /// A linked-list of `CacheBmp` associated with this image.
    /// Elements are linked by `CacheBmp::link_img`.
    bmps: ListHead<BmpPtr>,
}

#[derive(Debug)]
struct CacheBmp {
    /// A pointer to a `CacheImg` in `Cache::imgs`.
    img: ImgPtr,
    dpi_scale: DpiScale,
    bmp: Bmp,
    /// Links `CacheBmp`s associated with the same `ImgInner` together.
    link_img: Option<Link<BmpPtr>>,
    /// Links `CacheBmp`s having the same `DpiScale` together.
    link_dpi_scale: Option<Link<BmpPtr>>,
}

impl Cache {
    /// Construct a `Cache`.
    ///
    /// # Safety
    ///
    /// `ImgPtr` generated by the constructed `Cache` must not be used with
    /// other instances of `Cache`.
    const unsafe fn new() -> Self {
        Self {
            imgs: LeakyPool::new(),
            bmps: LeakyPool::new(),
            dpi_scales: Vec::new(),
        }
    }

    fn dpi_scale_add_ref(&mut self, dpi_scale: DpiScale) {
        for cache_dpi_scale in self.dpi_scales.iter_mut() {
            if cache_dpi_scale.dpi_scale == dpi_scale {
                cache_dpi_scale.ref_count += 1;
                return;
            }
        }
        self.dpi_scales.push(CacheDpiScale {
            dpi_scale,
            ref_count: 1,
            bmps: Default::default(),
        });
    }

    fn dpi_scale_release(&mut self, dpi_scale: DpiScale) {
        let i = self.dpi_scale_find(dpi_scale).map(|ptr| ptr.0);

        let i = i.expect("unknown DPI scale value");

        {
            let cache_dpi_scale = &mut self.dpi_scales[i];
            cache_dpi_scale.ref_count -= 1;

            if cache_dpi_scale.ref_count > 0 {
                return;
            }

            // `ref_count` hit zero, destroy all associated bitmmaps
            if let Some(mut bmp_ptr) = cache_dpi_scale.bmps.first {
                // Iterate through elements in a circular linked list.
                let first_bmp_ptr = bmp_ptr;
                loop {
                    let (next, img_ptr);
                    {
                        let bmp = &self.bmps[bmp_ptr];
                        next = bmp.link_dpi_scale.unwrap().next;
                        img_ptr = bmp.img;
                    }

                    // Remove the `CacheBmp` from `CacheImg::bmps`
                    let img: &mut CacheImg = &mut self.imgs[img_ptr];
                    img.bmps
                        .accessor_mut(&mut self.bmps, |bmp| &mut bmp.link_img)
                        .remove(bmp_ptr);

                    // No need to unlink `link_dpi_scale`; all bitmaps in the list
                    // are deleted in this loop anyway

                    // Delete `CacheBmp`
                    self.bmps.deallocate(bmp_ptr);

                    // Find the next bitmap
                    if next == first_bmp_ptr {
                        break;
                    } else {
                        bmp_ptr = next;
                    }
                }
            }
        }

        // Delete `CacheDpiScale`
        self.dpi_scales.swap_remove(i);
    }

    fn dpi_scale_find(&mut self, dpi_scale: DpiScale) -> Option<CacheDpiScalePtr> {
        self.dpi_scales
            .iter()
            .enumerate()
            .position(|(_, e)| e.dpi_scale == dpi_scale)
            .map(CacheDpiScalePtr)
    }

    fn img_add(&mut self) -> ImgPtr {
        self.imgs.allocate(CacheImg {
            bmps: Default::default(),
        })
    }

    fn img_remove(&mut self, img: ImgPtr) {
        // Destroy all associated bitmmaps
        if let Some(mut bmp_ptr) = self.imgs[img].bmps.first {
            // Iterate through elements in a circular linked list.
            let first_bmp_ptr = bmp_ptr;
            loop {
                let (next, dpi_scale);
                {
                    let bmp = &self.bmps[bmp_ptr];
                    next = bmp.link_img.unwrap().next;
                    dpi_scale = bmp.dpi_scale;
                }

                // Remove the `CacheBmp` from `CacheDpiScale::bmps`
                let cache_dpi_scale = self
                    .dpi_scales
                    .iter_mut()
                    .find(|e| e.dpi_scale == dpi_scale)
                    .unwrap();
                cache_dpi_scale
                    .bmps
                    .accessor_mut(&mut self.bmps, |bmp| &mut bmp.link_dpi_scale)
                    .remove(bmp_ptr);

                // No need to unlink `link_img`; all bitmaps in the list
                // are deleted in this loop anyway

                // Delete `CacheBmp`
                self.bmps.deallocate(bmp_ptr);

                // Find the next bitmap
                if next == first_bmp_ptr {
                    break;
                } else {
                    bmp_ptr = next;
                }
            }
        }

        self.imgs.deallocate(img);
    }

    fn img_find_bmp(&self, img: ImgPtr, dpi_scale: DpiScale) -> Option<&Bmp> {
        let cache_img = &self.imgs[img];

        let bmps = cache_img.bmps.accessor(&self.bmps, |bmp| &bmp.link_img);

        bmps.iter()
            .find(|(_, cache_bmp)| cache_bmp.dpi_scale == dpi_scale)
            .map(|(_, cache_bmp)| &cache_bmp.bmp)
    }

    fn img_add_bmp(&mut self, img: ImgPtr, dpi_scale: CacheDpiScalePtr, bmp: Bmp) -> &Bmp {
        let bmp_ptr = self.bmps.allocate(CacheBmp {
            img,
            dpi_scale: self.dpi_scales[dpi_scale.0].dpi_scale,
            bmp,
            link_img: None,
            link_dpi_scale: None,
        });

        // Add `bmp_ptr` to `CacheDpiScale::bmps`
        self.dpi_scales[dpi_scale.0]
            .bmps
            .accessor_mut(&mut self.bmps, |bmp| &mut bmp.link_dpi_scale)
            .push_back(bmp_ptr);

        // Add `bmp_ptr` to `CacheImg::bmps`
        self.imgs[img]
            .bmps
            .accessor_mut(&mut self.bmps, |bmp| &mut bmp.link_img)
            .push_back(bmp_ptr);

        &self.bmps[bmp_ptr].bmp
    }
}

/// A validated DPI scale value, fully supporting `Eq` and `Hash`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct DpiScale(u32);

quick_error! {
    #[derive(Debug)]
    enum DpiScaleError {
        OutOfRange {}
    }
}

impl DpiScale {
    fn new(x: f32) -> Result<Self, DpiScaleError> {
        if x.is_finite() && x > 0.0 {
            Ok(Self(x.to_bits()))
        } else {
            Err(DpiScaleError::OutOfRange)
        }
    }

    fn value(self) -> f32 {
        <f32>::from_bits(self.0)
    }
}

impl fmt::Debug for DpiScale {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("DpiScale").field(&self.value()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::super::BitmapImg;
    use super::*;
    use tcw3_pal::prelude::*;

    #[test]
    fn dpi_scales() {
        let mut cache = unsafe { Cache::new() };

        let scale1 = DpiScale::new(1.0).unwrap();
        let scale2 = DpiScale::new(2.0).unwrap();

        assert_eq!(cache.dpi_scales.len(), 0);
        assert!(cache.dpi_scale_find(scale1).is_none());
        assert!(cache.dpi_scale_find(scale2).is_none());

        cache.dpi_scale_add_ref(scale1);
        assert_eq!(cache.dpi_scales.len(), 1);
        assert!(cache.dpi_scale_find(scale1).is_some());
        assert!(cache.dpi_scale_find(scale2).is_none());

        cache.dpi_scale_add_ref(scale2);
        assert_eq!(cache.dpi_scales.len(), 2);
        assert!(cache.dpi_scale_find(scale1).is_some());
        assert!(cache.dpi_scale_find(scale2).is_some());

        cache.dpi_scale_add_ref(scale2);
        assert_eq!(cache.dpi_scales.len(), 2);
        assert!(cache.dpi_scale_find(scale1).is_some());
        assert!(cache.dpi_scale_find(scale2).is_some());

        cache.dpi_scale_release(scale1);
        assert_eq!(cache.dpi_scales.len(), 1);
        assert!(cache.dpi_scale_find(scale1).is_none());
        assert!(cache.dpi_scale_find(scale2).is_some());

        cache.dpi_scale_release(scale2);
        assert_eq!(cache.dpi_scales.len(), 1);
        assert!(cache.dpi_scale_find(scale1).is_none());
        assert!(cache.dpi_scale_find(scale2).is_some());

        cache.dpi_scale_release(scale2);
        assert_eq!(cache.dpi_scales.len(), 0);
        assert!(cache.dpi_scale_find(scale1).is_none());
        assert!(cache.dpi_scale_find(scale2).is_none());
    }

    #[test]
    fn imgs() {
        let mut cache = unsafe { Cache::new() };

        let bmp = tcw3_pal::BitmapBuilder::new([1, 1]).into_bitmap();
        let bmp = BitmapImg::new(bmp, 1.0);

        let scale1 = DpiScale::new(1.0).unwrap();
        let scale2 = DpiScale::new(2.0).unwrap();

        let img_ptr = cache.img_add();
        assert!(cache.img_find_bmp(img_ptr, scale1).is_none());
        assert!(cache.img_find_bmp(img_ptr, scale2).is_none());

        cache.dpi_scale_add_ref(scale1);
        cache.dpi_scale_add_ref(scale2);
        let scale1ptr = cache.dpi_scale_find(scale1).unwrap();
        let scale2ptr = cache.dpi_scale_find(scale2).unwrap();

        cache.img_add_bmp(img_ptr, scale1ptr, bmp.new_bmp(1.0));
        assert!(cache.img_find_bmp(img_ptr, scale1).is_some());
        assert!(cache.img_find_bmp(img_ptr, scale2).is_none());

        cache.img_add_bmp(img_ptr, scale2ptr, bmp.new_bmp(2.0));
        assert!(cache.img_find_bmp(img_ptr, scale1).is_some());
        assert!(cache.img_find_bmp(img_ptr, scale2).is_some());

        cache.dpi_scale_release(scale2);

        assert!(cache.img_find_bmp(img_ptr, scale1).is_some());
        assert!(cache.img_find_bmp(img_ptr, scale2).is_none());
    }
}
