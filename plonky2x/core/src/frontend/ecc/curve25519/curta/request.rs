use curta::chip::ec::EllipticCurve;
use curta::chip::field::parameters::FieldParameters;
use serde::{Deserialize, Serialize};

use crate::frontend::curta::ec::point::{AffinePointVariable, CompressedEdwardsYVariable};
use crate::frontend::num::nonnative::nonnative::NonNativeVariable;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EcOpRequestType {
    Add,
    ScalarMul,
    Decompress,
    IsValid,
}

/// A request for a EC OP computation.
#[derive(Debug, Clone)]
pub enum EcOpRequest<E: EllipticCurve, FF: FieldParameters> {
    /// Add
    Add(Box<AffinePointVariable<E>>, Box<AffinePointVariable<E>>),
    /// Scalar Mul
    ScalarMul(Box<NonNativeVariable<FF>>, Box<AffinePointVariable<E>>),
    /// Decompress
    Decompress(Box<CompressedEdwardsYVariable>),
    /// IsValid
    IsValid(Box<AffinePointVariable<E>>),
}

impl<E: EllipticCurve, FF: FieldParameters> EcOpRequest<E, FF> {
    /// Returns the type of the request.
    pub const fn req_type(&self) -> EcOpRequestType {
        match self {
            EcOpRequest::Add(_, _) => EcOpRequestType::Add,
            EcOpRequest::ScalarMul(_, _) => EcOpRequestType::ScalarMul,
            EcOpRequest::Decompress(_) => EcOpRequestType::Decompress,
            EcOpRequest::IsValid(_) => EcOpRequestType::IsValid,
        }
    }
}
