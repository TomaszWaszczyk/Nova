//! This module provides an implementation of a commitment engine
use crate::{
  errors::NovaError,
  traits::{
    commitment::{
      CommitmentEngineTrait, CommitmentGensTrait, CommitmentTrait, CompressedCommitmentTrait,
    },
    AbsorbInROTrait, AppendToTranscriptTrait, CompressedGroup, Group, ROTrait,
    TranscriptEngineTrait,
  },
};
use core::{
  fmt::Debug,
  marker::PhantomData,
  ops::{Add, AddAssign, Mul, MulAssign},
};
use ff::{Field, PrimeField};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// A type that holds commitment generators
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitmentGens<G: Group> {
  gens: Vec<G::PreprocessedGroupElement>,
  _p: PhantomData<G>,
}

/// A type that holds a commitment
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Commitment<G: Group> {
  pub(crate) comm: G,
}

/// A type that holds a compressed commitment
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct CompressedCommitment<C: CompressedGroup> {
  comm: C,
}

impl<G: Group> CommitmentGensTrait<G> for CommitmentGens<G> {
  type Commitment = Commitment<G>;
  type CompressedCommitment = CompressedCommitment<G::CompressedGroupElement>;

  fn new(label: &'static [u8], n: usize) -> Self {
    CommitmentGens {
      gens: G::from_label(label, n.next_power_of_two()),
      _p: Default::default(),
    }
  }

  fn len(&self) -> usize {
    self.gens.len()
  }

  fn commit(&self, v: &[G::Scalar]) -> Self::Commitment {
    assert!(self.gens.len() >= v.len());
    Commitment {
      comm: G::vartime_multiscalar_mul(v, &self.gens[..v.len()]),
    }
  }
}

impl<G: Group> CommitmentTrait<G> for Commitment<G> {
  type CompressedCommitment = CompressedCommitment<G::CompressedGroupElement>;

  fn compress(&self) -> CompressedCommitment<G::CompressedGroupElement> {
    CompressedCommitment {
      comm: self.comm.compress(),
    }
  }

  fn to_coordinates(&self) -> (G::Base, G::Base, bool) {
    self.comm.to_coordinates()
  }
}

impl<G: Group> Default for Commitment<G> {
  fn default() -> Self {
    Commitment { comm: G::zero() }
  }
}

impl<C: CompressedGroup> CompressedCommitmentTrait<C> for CompressedCommitment<C> {
  type Commitment = Commitment<C::GroupElement>;

  fn decompress(&self) -> Result<Self::Commitment, NovaError> {
    let comm = self.comm.decompress();
    if comm.is_none() {
      return Err(NovaError::DecompressionError);
    }
    Ok(Commitment {
      comm: comm.unwrap(),
    })
  }
}

impl<G: Group> AppendToTranscriptTrait<G> for Commitment<G> {
  fn append_to_transcript(&self, label: &'static [u8], transcript: &mut G::TE) {
    let (x, y, is_infinity) = self.comm.to_coordinates();
    let is_infinity_byte = if is_infinity { 0u8 } else { 1u8 };
    let bytes = [
      x.to_repr().as_ref(),
      y.to_repr().as_ref(),
      &[is_infinity_byte],
    ]
    .concat();
    transcript.absorb_bytes(label, &bytes);
  }
}

impl<G: Group> AbsorbInROTrait<G> for Commitment<G> {
  fn absorb_in_ro(&self, ro: &mut G::RO) {
    let (x, y, is_infinity) = self.comm.to_coordinates();
    ro.absorb(x);
    ro.absorb(y);
    ro.absorb(if is_infinity {
      G::Base::one()
    } else {
      G::Base::zero()
    });
  }
}

impl<C: CompressedGroup> AppendToTranscriptTrait<C::GroupElement> for CompressedCommitment<C> {
  fn append_to_transcript(
    &self,
    label: &'static [u8],
    transcript: &mut <C::GroupElement as Group>::TE,
  ) {
    let comm = self.decompress().unwrap();
    <Commitment<C::GroupElement> as AppendToTranscriptTrait<C::GroupElement>>::append_to_transcript(
      &comm, label, transcript,
    );
  }
}

impl<G: Group> MulAssign<G::Scalar> for Commitment<G> {
  fn mul_assign(&mut self, scalar: G::Scalar) {
    let result = (self as &Commitment<G>).comm * scalar;
    *self = Commitment { comm: result };
  }
}

impl<'a, 'b, G: Group> Mul<&'b G::Scalar> for &'a Commitment<G> {
  type Output = Commitment<G>;
  fn mul(self, scalar: &'b G::Scalar) -> Commitment<G> {
    Commitment {
      comm: self.comm * scalar,
    }
  }
}

impl<G: Group> Mul<G::Scalar> for Commitment<G> {
  type Output = Commitment<G>;

  fn mul(self, scalar: G::Scalar) -> Commitment<G> {
    Commitment {
      comm: self.comm * scalar,
    }
  }
}

impl<'b, G: Group> AddAssign<&'b Commitment<G>> for Commitment<G> {
  fn add_assign(&mut self, other: &'b Commitment<G>) {
    let result = (self as &Commitment<G>).comm + other.comm;
    *self = Commitment { comm: result };
  }
}

impl<'a, 'b, G: Group> Add<&'b Commitment<G>> for &'a Commitment<G> {
  type Output = Commitment<G>;
  fn add(self, other: &'b Commitment<G>) -> Commitment<G> {
    Commitment {
      comm: self.comm + other.comm,
    }
  }
}

macro_rules! define_add_variants {
  (G = $g:path, LHS = $lhs:ty, RHS = $rhs:ty, Output = $out:ty) => {
    impl<'b, G: $g> Add<&'b $rhs> for $lhs {
      type Output = $out;
      fn add(self, rhs: &'b $rhs) -> $out {
        &self + rhs
      }
    }

    impl<'a, G: $g> Add<$rhs> for &'a $lhs {
      type Output = $out;
      fn add(self, rhs: $rhs) -> $out {
        self + &rhs
      }
    }

    impl<G: $g> Add<$rhs> for $lhs {
      type Output = $out;
      fn add(self, rhs: $rhs) -> $out {
        &self + &rhs
      }
    }
  };
}

macro_rules! define_add_assign_variants {
  (G = $g:path, LHS = $lhs:ty, RHS = $rhs:ty) => {
    impl<G: $g> AddAssign<$rhs> for $lhs {
      fn add_assign(&mut self, rhs: $rhs) {
        *self += &rhs;
      }
    }
  };
}

define_add_assign_variants!(G = Group, LHS = Commitment<G>, RHS = Commitment<G>);
define_add_variants!(G = Group, LHS = Commitment<G>, RHS = Commitment<G>, Output = Commitment<G>);

/// Provides a commitment engine
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitmentEngine<G: Group> {
  _p: PhantomData<G>,
}

impl<G: Group> CommitmentEngineTrait<G> for CommitmentEngine<G> {
  type CommitmentGens = CommitmentGens<G>;
  type Commitment = Commitment<G>;
  type CompressedCommitment = CompressedCommitment<G::CompressedGroupElement>;

  fn commit(gens: &Self::CommitmentGens, v: &[G::Scalar]) -> Self::Commitment {
    gens.commit(v)
  }
}

pub(crate) trait CommitmentGensExtTrait<G: Group>: CommitmentGensTrait<G> {
  type CE: CommitmentEngineTrait<G>;

  /// Splits the commitment key into two pieces at a specified point
  fn split_at(&self, n: usize) -> (Self, Self)
  where
    Self: Sized;

  /// Combines two commitment keys into one
  fn combine(&self, other: &Self) -> Self;

  /// Folds the two commitment keys into one using the provided weights
  fn fold(&self, w1: &G::Scalar, w2: &G::Scalar) -> Self;

  /// Scales the commitment key using the provided scalar
  fn scale(&self, r: &G::Scalar) -> Self;

  /// Reinterprets commitments as commitment keys
  fn reinterpret_commitments_as_gens(
    c: &[<<Self as CommitmentGensExtTrait<G>>::CE as CommitmentEngineTrait<G>>::CompressedCommitment],
  ) -> Result<Self, NovaError>
  where
    Self: Sized;
}

impl<G: Group> CommitmentGensExtTrait<G> for CommitmentGens<G> {
  type CE = CommitmentEngine<G>;

  fn split_at(&self, n: usize) -> (CommitmentGens<G>, CommitmentGens<G>) {
    (
      CommitmentGens {
        gens: self.gens[0..n].to_vec(),
        _p: Default::default(),
      },
      CommitmentGens {
        gens: self.gens[n..].to_vec(),
        _p: Default::default(),
      },
    )
  }

  fn combine(&self, other: &CommitmentGens<G>) -> CommitmentGens<G> {
    let gens = {
      let mut c = self.gens.clone();
      c.extend(other.gens.clone());
      c
    };
    CommitmentGens {
      gens,
      _p: Default::default(),
    }
  }

  // combines the left and right halves of `self` using `w1` and `w2` as the weights
  fn fold(&self, w1: &G::Scalar, w2: &G::Scalar) -> CommitmentGens<G> {
    let w = vec![*w1, *w2];
    let (L, R) = self.split_at(self.len() / 2);

    let gens = (0..self.len() / 2)
      .into_par_iter()
      .map(|i| {
        let bases = [L.gens[i].clone(), R.gens[i].clone()].to_vec();
        G::vartime_multiscalar_mul(&w, &bases).preprocessed()
      })
      .collect();

    CommitmentGens {
      gens,
      _p: Default::default(),
    }
  }

  /// Scales each element in `self` by `r`
  fn scale(&self, r: &G::Scalar) -> Self {
    let gens_scaled = self
      .gens
      .clone()
      .into_par_iter()
      .map(|g| G::vartime_multiscalar_mul(&[*r], &[g]).preprocessed())
      .collect();

    CommitmentGens {
      gens: gens_scaled,
      _p: Default::default(),
    }
  }

  /// reinterprets a vector of commitments as a set of generators
  fn reinterpret_commitments_as_gens(
    c: &[CompressedCommitment<G::CompressedGroupElement>],
  ) -> Result<Self, NovaError> {
    let d = (0..c.len())
      .into_par_iter()
      .map(|i| c[i].decompress())
      .collect::<Result<Vec<Commitment<G>>, NovaError>>()?;
    let gens = (0..d.len())
      .into_par_iter()
      .map(|i| d[i].comm.preprocessed())
      .collect();
    Ok(CommitmentGens {
      gens,
      _p: Default::default(),
    })
  }
}
