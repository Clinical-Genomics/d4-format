use d4::ptab::DecodeResult;
use d4::stab::SecondaryTablePartReader;
use d4::D4TrackReader;
use pyo3::iter::IterNextOutput;
use pyo3::{PyIterProtocol, prelude::*};
use std::io::Result;

/// Value iterator for D4 file
#[pyclass]
pub struct D4Iter {
    _inner: D4TrackReader,
    iter: Box<dyn Iterator<Item = i32> + Send + 'static>,
}

impl D4Iter {
	pub(crate) fn new(mut inner: D4TrackReader, chr: &str, left: u32, right: u32) -> PyResult<Self> {
        let partition = inner.split(None)?;

        let chr = chr.to_string();

        let iter = partition
            .into_iter()
            .map(move |(mut ptab, mut stab)| {
                let (part_chr, begin, end) = ptab.region();
                let part_chr = part_chr.to_string();
                let pd = ptab.to_codec();
                (if part_chr != chr {
                    0..0
                } else {
                    left.max(begin)..right.min(end)
                })
                .map(move |pos| match pd.decode(pos as usize) {
                    DecodeResult::Definitely(value) => value,
                    DecodeResult::Maybe(value) => {
                        if let Some(st_value) = stab.decode(pos) {
                            st_value
                        } else {
                            value
                        }
                    }
                })
            })
            .flatten();
        Ok(D4Iter {
            _inner: inner,
            iter: Box::new(iter),
        })
    }
}

#[pyproto]
impl PyIterProtocol for D4Iter {
    fn __iter__(slf: PyRefMut<Self>) -> Result<PyRefMut<Self>> {
        Ok(slf)
    }
    fn __next__(mut slf: PyRefMut<Self>) -> IterNextOutput<i32, &'static str> {
        if let Some(next) = slf.iter.next() {
            IterNextOutput::Yield(next)
        } else {
            IterNextOutput::Return("Ended")
        }
    }
}