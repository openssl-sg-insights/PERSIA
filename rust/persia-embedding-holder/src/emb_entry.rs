use persia_libs::{
    half::{f16, prelude::*},
    ndarray::Array1,
    ndarray_rand::rand_distr::{Gamma, Normal, Poisson, Uniform},
    ndarray_rand::RandomExt,
    rand::prelude::SmallRng,
    rand::SeedableRng,
    serde::{self, Deserialize, Serialize},
};

use persia_embedding_config::InitializationMethod;
use persia_speedy::{Readable, Writable};

use crate::eviction_map::EvictionMapValue;

#[derive(Serialize, Deserialize, Readable, Writable, Debug)]
#[serde(crate = "self::serde")]
pub enum Entry {
    F32(Vec<f32>),
    F16(Vec<f16>),
}

impl Entry {
    fn get_f32_mut(&mut self) -> F32EntryMutSlice {
        let replica_data = match self {
            Entry::F16(val) => Some(val.as_slice().to_f32_vec()),
            Entry::F32(_) => None,
        };
        // let ptr = self.as_mut_ref().ptr;
        F32EntryMutSlice {
            replica_data,
            // ptr: self.().ptr,
            ptr: self as *mut _,
        }
    }
}
pub struct F32EntryMutSlice {
    ptr: *mut Entry,
    replica_data: Option<Vec<f32>>,
}

impl F32EntryMutSlice {
    fn as_slice_mut(&mut self) -> &mut [f32] {
        match self.replica_data.as_mut() {
            Some(val) => val.as_mut_slice(),
            None => {
                let source = unsafe { &mut *self.ptr };

                match source {
                    Entry::F16(_) => panic!("f16 data should not replica..."),
                    Entry::F32(val) => val.as_mut_slice(),
                }
            }
        }
    }
}

impl Drop for F32EntryMutSlice {
    fn drop(&mut self) {
        let source = unsafe { &mut *self.ptr };

        match source {
            Entry::F16(_) => {
                let f32_vec = self.replica_data.take().unwrap();
                let f16_entry = Entry::F16(Vec::from_f32_slice(f32_vec.as_slice()));
                *source = f16_entry;
            }
            Entry::F32(_) => {}
        }
    }
}

#[derive(Serialize, Deserialize, Readable, Writable, Clone, Debug)]
#[serde(crate = "self::serde")]
pub struct HashMapEmbeddingEntry {
    inner: Vec<f32>,
    embedding_dim: usize,
    sign: u64,
}

impl HashMapEmbeddingEntry {
    pub fn new(
        initialization_method: &InitializationMethod,
        dim: usize,
        require_space: usize,
        seed: u64,
        sign: u64,
    ) -> Self {
        let emb = {
            let mut rng = SmallRng::seed_from_u64(seed);
            match initialization_method {
                InitializationMethod::BoundedUniform(x) => {
                    Array1::random_using((dim,), Uniform::new(x.lower, x.upper), &mut rng)
                }
                InitializationMethod::BoundedGamma(x) => {
                    Array1::random_using((dim,), Gamma::new(x.shape, x.scale).unwrap(), &mut rng)
                }
                InitializationMethod::BoundedPoisson(x) => {
                    Array1::random_using((dim,), Poisson::new(x.lambda).unwrap(), &mut rng)
                }
                InitializationMethod::BoundedNormal(x) => Array1::random_using(
                    (dim,),
                    Normal::new(x.mean, x.standard_deviation).unwrap(),
                    &mut rng,
                ),
                _ => panic!(
                    "unsupported initialization method for hashmap impl: {:?}",
                    initialization_method
                ),
            }
        };

        let mut inner = emb.into_raw_vec();
        if require_space > 0 {
            inner.resize(inner.len() + require_space, 0.0_f32);
        }
        Self {
            inner,
            embedding_dim: dim,
            sign,
        }
    }

    pub fn new_empty(dim: usize, require_space: usize, sign: u64) -> Self {
        Self {
            inner: vec![0f32; dim + require_space],
            embedding_dim: dim,
            sign,
        }
    }

    pub fn from_emb(emb: Vec<f32>, sign: u64) -> Self {
        let embedding_dim = emb.len();
        Self {
            inner: emb,
            embedding_dim,
            sign,
        }
    }

    pub fn from_emb_and_opt(emb: Vec<f32>, opt: &[f32], sign: u64) -> Self {
        let embedding_dim = emb.len();
        let mut inner = emb;
        inner.extend_from_slice(opt);
        Self {
            inner,
            embedding_dim,
            sign,
        }
    }

    pub fn copy_from_other(&mut self, other: &Self) -> bool {
        if self.embedding_dim() != other.embedding_dim() {
            return false;
        }
        for (dst, src) in self.inner.iter_mut().zip(other.inner.iter()) {
            *dst = *src;
        }
        return true;
    }

    pub fn as_mut_emb_entry_slice(&mut self) -> &mut [f32] {
        self.inner.as_mut_slice()
    }

    pub fn as_emb_entry_slice(&self) -> &[f32] {
        self.inner.as_slice()
    }

    pub fn inner_size(&self) -> usize {
        self.inner.len()
    }

    pub fn dim(&self) -> usize {
        self.embedding_dim
    }

    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    pub fn emb(&self) -> &[f32] {
        &self.inner[..self.embedding_dim()]
    }

    pub fn emb_mut(&mut self) -> &mut [f32] {
        let dim = self.embedding_dim();
        &mut self.inner[..dim]
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    pub fn opt(&self) -> &[f32] {
        &self.inner[self.embedding_dim()..]
    }

    pub fn opt_mut(&mut self) -> &mut [f32] {
        let dim = self.embedding_dim();
        &mut self.inner[dim..]
    }

    pub fn emb_and_opt_mut(&mut self) -> (&mut [f32], &mut [f32]) {
        let dim = self.embedding_dim();
        self.inner.split_at_mut(dim)
    }

    pub fn sign(&self) -> u64 {
        self.sign
    }
}

impl EvictionMapValue<u64> for HashMapEmbeddingEntry {
    fn hashmap_key(&self) -> u64 {
        self.sign
    }
}
