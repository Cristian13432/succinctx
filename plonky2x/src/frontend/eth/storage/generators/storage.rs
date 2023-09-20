use core::fmt::Debug;
use core::marker::PhantomData;

use async_trait::async_trait;
use ethers::providers::Middleware;
use ethers::types::TransactionReceipt;
use plonky2::iop::generator::{GeneratedValues, SimpleGenerator};
use plonky2::iop::target::Target;
use plonky2::iop::witness::PartitionWitness;
use plonky2::plonk::circuit_data::CommonCircuitData;
use plonky2::util::serialization::{Buffer, IoResult, Read, Write};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::runtime::Runtime;

use crate::backend::circuit::PlonkParameters;
use crate::frontend::builder::CircuitBuilder;
use crate::frontend::eth::storage::utils::get_map_storage_location;
use crate::frontend::eth::storage::vars::{EthLog, EthLogVariable};
use crate::frontend::eth::utils::u256_to_h256_be;
use crate::frontend::eth::vars::AddressVariable;
use crate::frontend::generator::asynchronous::hint::AsyncHint;
use crate::frontend::uint::uint256::U256Variable;
use crate::frontend::vars::{Bytes32Variable, CircuitVariable, ValueStream};
use crate::utils::eth::get_provider;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthStorageProofHint<L: PlonkParameters<D>, const D: usize> {
    chain_id: u64,
    _phantom: PhantomData<L>,
}

impl<L: PlonkParameters<D>, const D: usize> EthStorageProofHint<L, D> {
    pub fn new(builder: &CircuitBuilder<L, D>) -> EthStorageProofHint<L, D> {
        let chain_id = builder.get_chain_id();
        EthStorageProofHint {
            chain_id,
            _phantom: PhantomData::<L>,
        }
    }
}

#[async_trait]
impl<L: PlonkParameters<D>, const D: usize> AsyncHint<L, D> for EthStorageProofHint<L, D> {
    async fn hint(
        &self,
        input_stream: &mut ValueStream<L, D>,
        output_stream: &mut ValueStream<L, D>,
    ) {
        let block_hash = input_stream.read_value::<Bytes32Variable>();
        let address = input_stream.read_value::<AddressVariable>();
        let location = input_stream.read_value::<Bytes32Variable>();

        let provider = get_provider(self.chain_id);
        let result = provider
            .get_proof(address, vec![location], Some(block_hash.into()))
            .await
            .expect("Failed to get proof");
        let value = u256_to_h256_be(result.storage_proof[0].value);
        output_stream.write_value::<Bytes32Variable>(value);
    }
}

#[derive(Debug, Clone)]
pub struct EthStorageKeyGenerator<L: PlonkParameters<D>, const D: usize> {
    mapping_location: U256Variable,
    map_key: Bytes32Variable,
    pub value: Bytes32Variable,
    _phantom: PhantomData<L>,
}

impl<L: PlonkParameters<D>, const D: usize> EthStorageKeyGenerator<L, D> {
    pub fn new(
        builder: &mut CircuitBuilder<L, D>,
        mapping_location: U256Variable,
        map_key: Bytes32Variable,
    ) -> EthStorageKeyGenerator<L, D> {
        let value = builder.init::<Bytes32Variable>();
        EthStorageKeyGenerator {
            mapping_location,
            map_key,
            value,
            _phantom: PhantomData,
        }
    }

    pub fn id() -> String {
        "EthStorageKeyGenerator".to_string()
    }
}

impl<L: PlonkParameters<D>, const D: usize> SimpleGenerator<L::Field, D>
    for EthStorageKeyGenerator<L, D>
{
    fn id(&self) -> String {
        Self::id()
    }

    fn dependencies(&self) -> Vec<Target> {
        let mut targets = Vec::new();
        targets.extend(self.mapping_location.targets());
        targets.extend(self.map_key.targets());
        targets
    }

    fn run_once(
        &self,
        witness: &PartitionWitness<L::Field>,
        buffer: &mut GeneratedValues<L::Field>,
    ) {
        let mapping_location = self.mapping_location.get(witness);
        let map_key = self.map_key.get(witness);

        let location = get_map_storage_location(mapping_location.as_u128(), map_key);
        self.value.set(buffer, location);
    }

    #[allow(unused_variables)]
    fn serialize(
        &self,
        dst: &mut Vec<u8>,
        common_data: &CommonCircuitData<L::Field, D>,
    ) -> IoResult<()> {
        dst.write_target_vec(&self.mapping_location.targets())?;
        dst.write_target_vec(&self.map_key.targets())?;
        dst.write_target_vec(&self.value.targets())
    }

    #[allow(unused_variables)]
    fn deserialize(
        src: &mut Buffer,
        common_data: &CommonCircuitData<L::Field, D>,
    ) -> IoResult<Self> {
        let mapping_location_targets = src.read_target_vec()?;
        let mapping_location = U256Variable::from_targets(&mapping_location_targets);

        let map_key_targets = src.read_target_vec()?;
        let map_key = Bytes32Variable::from_targets(&map_key_targets);

        let value_targets = src.read_target_vec()?;
        let value = Bytes32Variable::from_targets(&value_targets);

        Ok(Self {
            mapping_location,
            map_key,
            value,
            _phantom: PhantomData::<L>,
        })
    }
}

#[derive(Debug, Clone)]
pub struct EthLogGenerator<L: PlonkParameters<D>, const D: usize> {
    transaction_hash: Bytes32Variable,
    block_hash: Bytes32Variable,
    log_index: u64,
    pub value: EthLogVariable,
    chain_id: u64,
    _phantom: PhantomData<L>,
}

impl<L: PlonkParameters<D>, const D: usize> EthLogGenerator<L, D> {
    pub fn new(
        builder: &mut CircuitBuilder<L, D>,
        transaction_hash: Bytes32Variable,
        block_hash: Bytes32Variable,
        log_index: u64,
    ) -> EthLogGenerator<L, D> {
        let chain_id = builder.get_chain_id();
        let value = builder.init::<EthLogVariable>();
        EthLogGenerator {
            transaction_hash,
            block_hash,
            log_index,
            value,
            chain_id,
            _phantom: PhantomData,
        }
    }

    pub fn id() -> String {
        "EthLogGenerator".to_string()
    }
}

impl<L: PlonkParameters<D>, const D: usize> SimpleGenerator<L::Field, D> for EthLogGenerator<L, D> {
    fn id(&self) -> String {
        Self::id()
    }

    fn dependencies(&self) -> Vec<Target> {
        let mut targets = Vec::new();
        targets.extend(self.transaction_hash.targets());
        targets.extend(self.block_hash.targets());
        targets
    }

    fn run_once(
        &self,
        witness: &PartitionWitness<L::Field>,
        buffer: &mut GeneratedValues<L::Field>,
    ) {
        let transaction_hash = self.transaction_hash.get(witness);
        // block_hash is unused
        let _block_hash = self.block_hash.get(witness);

        let provider = get_provider(self.chain_id);
        let rt = Runtime::new().expect("failed to create tokio runtime");
        let result: TransactionReceipt = rt
            .block_on(async {
                provider
                    .get_transaction_receipt(transaction_hash)
                    .await
                    .expect("Failed to call get_transaction_receipt")
            })
            .expect("No transaction receipt found");

        let log = &result.logs[self.log_index as usize];
        let value = EthLog {
            address: log.address,
            topics: [log.topics[0], log.topics[1], log.topics[2]],
            data_hash: ethers::types::H256::from_slice(sha2::Sha256::digest(&log.data).as_ref()),
        };
        self.value.set(buffer, value);
    }

    #[allow(unused_variables)]
    fn serialize(
        &self,
        dst: &mut Vec<u8>,
        common_data: &CommonCircuitData<L::Field, D>,
    ) -> IoResult<()> {
        let chain_id_bytes = self.chain_id.to_be_bytes();
        dst.write_all(&chain_id_bytes)?;

        dst.write_target_vec(&self.transaction_hash.targets())?;
        dst.write_target_vec(&self.block_hash.targets())?;

        let log_index_bytes = self.log_index.to_be_bytes();
        dst.write_all(&log_index_bytes)?;

        dst.write_target_vec(&self.value.targets())
    }

    #[allow(unused_variables)]
    fn deserialize(
        src: &mut Buffer,
        common_data: &CommonCircuitData<L::Field, D>,
    ) -> IoResult<Self> {
        let mut chain_id_bytes = [0u8; 8];
        src.read_exact(&mut chain_id_bytes)?;
        let chain_id = u64::from_be_bytes(chain_id_bytes);

        let transaction_hash_targets = src.read_target_vec()?;
        let transaction_hash = Bytes32Variable::from_targets(&transaction_hash_targets);

        let block_hash_targets = src.read_target_vec()?;
        let block_hash = Bytes32Variable::from_targets(&block_hash_targets);

        let mut log_index_bytes = [0u8; 8];
        src.read_exact(&mut log_index_bytes)?;
        let log_index = u64::from_be_bytes(log_index_bytes);

        let value_targets = src.read_target_vec()?;
        let value = EthLogVariable::from_targets(&value_targets);

        Ok(Self {
            block_hash,
            transaction_hash,
            log_index,
            value,
            chain_id,
            _phantom: PhantomData::<L>,
        })
    }
}
