use curta::chip::ec::EllipticCurve;
use curta::chip::field::parameters::FieldParameters;

use super::accelerator::EcOpAccelerator;
use super::request::EcOpRequest;
use super::result_hint::EcOpResultHint;
use crate::frontend::curta::ec::point::AffinePointVariable;
use crate::prelude::{CircuitBuilder, PlonkParameters, VariableStream};

impl<L: PlonkParameters<D>, const D: usize> CircuitBuilder<L, D> {
    /// The constraints for an accelerated EC Ops computation using Curta.
    pub(crate) fn curta_constrain_ec_op<E: EllipticCurve, FF: FieldParameters>(
        &mut self,
        accelerator: EcOpAccelerator<E, FF>,
    ) {
        // Get all the digest values using the digest hint.
        for (request, response) in accelerator
            .ec_op_requests
            .iter()
            .zip(accelerator.ec_op_responses.iter())
        {
            let result_hint = EcOpResultHint::<E, FF>::new(request.req_type());
            let mut input_stream = VariableStream::new();

            match &request {
                EcOpRequest::Add(a, b) => {
                    input_stream.write(a);
                    input_stream.write(b);
                }
                EcOpRequest::ScalarMul(scalar, point) => {
                    input_stream.write(scalar);
                    input_stream.write(point);
                }
                EcOpRequest::Decompress(compressedPoint) => {
                    input_stream.write(compressedPoint);
                }
                EcOpRequest::IsValid(point) => {
                    input_stream.write(point);
                }
            }

            let output_stream = self.hint(input_stream, result_hint);

            match &request {
                EcOpRequest::Add(_, _)
                | EcOpRequest::ScalarMul(_, _)
                | EcOpRequest::Decompress(_) => {
                    let result = output_stream.read::<AffinePointVariable<E>>(self);
                    self.assert_is_equal(result, response.expect("response should not be None"));
                }
                EcOpRequest::IsValid(_) => {}
            }
        }
    }
}
