use miniserde::{de, make_place, ser, Error, Result};
use std::cell::UnsafeCell;

use super::Elem;

make_place!(Place);

impl<T> de::Visitor for Place<Elem<T>>
where
    T: de::Deserialize,
{
    fn null(&mut self) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).null()?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn boolean(&mut self, b: bool) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).boolean(b)?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn string(&mut self, s: &str) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).string(s)?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn negative(&mut self, n: i64) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).negative(n)?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn nonnegative(&mut self, n: u64) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).nonnegative(n)?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn float(&mut self, n: f64) -> Result<()> {
        let mut inner = None::<T>;
        de::Deserialize::begin(&mut inner).float(n)?;
        self.out = Some(Elem::new(inner.ok_or(Error)?));
        Ok(())
    }
    fn seq(&mut self) -> Result<Box<dyn de::Seq + '_>> {
        struct SeqState<'a, 'b, T> {
            out: &'a mut Option<Elem<T>>,
            inner_obj: UnsafeCell<Option<T>>,
            inner_seq: Option<Box<dyn de::Seq + 'b>>, // refers `obj`
        }

        impl<T> de::Seq for SeqState<'_, '_, T> {
            fn element(&mut self) -> Result<&mut dyn de::Visitor> {
                self.inner_seq.as_mut().ok_or(Error)?.element()
            }

            fn finish(&mut self) -> Result<()> {
                self.inner_seq.as_mut().ok_or(Error)?.finish()?;
                self.inner_seq = None; // must be dropped before `inner_obj`

                let inner_obj = unsafe { &mut *self.inner_obj.get() }.take();
                *self.out = Some(Elem::new(inner_obj.ok_or(Error)?));
                Ok(())
            }
        }

        impl<T> Drop for SeqState<'_, '_, T> {
            fn drop(&mut self) {
                self.inner_seq = None; // must be dropped before `inner_obj`
            }
        }

        let mut seq = Box::new(SeqState {
            out: &mut self.out,
            inner_obj: UnsafeCell::new(None),
            inner_seq: None,
        });

        // Erase the lifetime so that `inner_seq` can refer to `inner_obj`
        let inner_seq = de::Deserialize::begin(unsafe { &mut *seq.inner_obj.get() }).seq()?;
        seq.inner_seq = Some(inner_seq);

        Ok(seq)
    }
    fn map(&mut self) -> Result<Box<dyn de::Map + '_>> {
        struct MapState<'a, 'b, T> {
            out: &'a mut Option<Elem<T>>,
            inner_obj: UnsafeCell<Option<T>>,
            inner_seq: Option<Box<dyn de::Map + 'b>>, // refers `obj`
        }

        impl<T> de::Map for MapState<'_, '_, T> {
            fn key(&mut self, k: &str) -> Result<&mut dyn de::Visitor> {
                self.inner_seq.as_mut().ok_or(Error)?.key(k)
            }

            fn finish(&mut self) -> Result<()> {
                self.inner_seq.as_mut().ok_or(Error)?.finish()?;
                self.inner_seq = None; // must be dropped before `inner_obj`

                let inner_obj = unsafe { &mut *self.inner_obj.get() }.take();
                *self.out = Some(Elem::new(inner_obj.ok_or(Error)?));
                Ok(())
            }
        }

        impl<T> Drop for MapState<'_, '_, T> {
            fn drop(&mut self) {
                self.inner_seq = None; // must be dropped before `inner_obj`
            }
        }

        let mut seq = Box::new(MapState {
            out: &mut self.out,
            inner_obj: UnsafeCell::new(None),
            inner_seq: None,
        });

        // Erase the lifetime so that `inner_seq` can refer to `inner_obj`
        let inner_seq = de::Deserialize::begin(unsafe { &mut *seq.inner_obj.get() }).map()?;
        seq.inner_seq = Some(inner_seq);

        Ok(seq)
    }
}

impl<T> de::Deserialize for Elem<T>
where
    T: de::Deserialize,
{
    fn begin(out: &mut Option<Self>) -> &mut dyn de::Visitor {
        Place::new(out)
    }
}

impl<T> ser::Serialize for Elem<T>
where
    T: ser::Serialize,
{
    fn begin(&self) -> ser::Fragment {
        (**self).begin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miniserde::{json, Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct Example {
        code: u32,
        message: String,
        payload: Vec<u32>,
    }

    #[test]
    fn serialize() {
        let reference = Example {
            code: 200,
            message: "reminiscent of Serde".to_owned(),
            payload: vec![0, 118, 999, 881, 119, 7253],
        };

        let input = Elem::new(reference.clone());
        let j = json::to_string(&input);
        let output: Example = json::from_str(&j).unwrap();

        assert_eq!(output, reference);
    }

    #[test]
    fn deserialize() {
        let reference = Example {
            code: 200,
            message: "reminiscent of Serde".to_owned(),
            payload: vec![0, 118, 999, 881, 119, 7253],
        };

        let input = reference.clone();
        let j = json::to_string(&input);
        let output: Elem<Example> = json::from_str(&j).unwrap();

        assert_eq!(*output, reference);
    }
}
