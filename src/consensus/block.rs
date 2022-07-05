use std::io;

use log::debug;

use super::{Metadata, StreamletMetadata, OuroborosMetadata, BLOCK_VERSION};
use crate::{
    crypto::{address::Address, keypair::PublicKey, schnorr::Signature},
    impl_vec, net,
    tx::Transaction,
    util::{
        serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
        time::Timestamp,
    },
    Result,
};

/// This struct represents a tuple of the form (`v`, `st`, `e`, `sl`, `txs`, `metadata`).
/// The transactions here are stored as hashes, which serve as pointers to
/// the actual transaction data in the blockchain database.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block version
    pub v: u8,
    /// Previous block hash
    pub st: blake3::Hash,
    /// Epoch
    pub e: u64,
    /// Slot uid
    pub sl: u64,
    /// Transaction hashes
    pub txs: Vec<blake3::Hash>,
    /// Additional block information
    pub metadata: Metadata,
}

impl Block {
    pub fn new(
        st: blake3::Hash,
        e: u64,
        sl: u64,
        txs: Vec<blake3::Hash>,
        metadata: Metadata,
    ) -> Self {
        let v = *BLOCK_VERSION;
        Self { v, st, e, sl, txs, metadata }
    }

    /// Generate the genesis block.
    pub fn genesis_block(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Self {
        let eta : [u8; 32] = *blake3::hash(b"let there be dark!").as_bytes();
        let metadata =
            Metadata::new(genesis_ts, eta);

        Self::new(genesis_data, 0, 0, vec![], metadata)
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct BlockOrder {
    /// Slot UID
    pub sl: u64,
    /// Blockhash of that slot
    pub block: blake3::Hash,
}

impl net::Message for BlockOrder {
    fn name() -> &'static str {
        "blockorder"
    }
}

/// Structure representing full block data.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockInfo {
    /// Block version
    pub v: u8,
    /// Previous block hash
    pub st: blake3::Hash,
    /// Epoch
    pub e: u64,
    /// Slot uid
    pub sl: u64,
    /// Transactions payload
    pub txs: Vec<Transaction>,
    /// Additional proposal information
    pub metadata: Metadata,
    // Proposal information used by Streamlet consensus
    pub sm: StreamletMetadata,
}

impl BlockInfo {
    pub fn new(
        st: blake3::Hash,
        e: u64,
        sl: u64,
        txs: Vec<Transaction>,
        metadata: Metadata,
        sm: StreamletMetadata
    ) -> Self {
        let v = *BLOCK_VERSION;
        Self { v, st, e, sl, txs, metadata, sm}
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        let block: Block = self.clone().into();
        block.blockhash()
    }
}

impl From<BlockInfo> for Block {
    fn from(b: BlockInfo) -> Self {
        let txids = b.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        Self { v: b.v, st: b.st, e: b.e, sl: b.sl, txs: txids, metadata: b.metadata }
    }
}

impl net::Message for BlockInfo {
    fn name() -> &'static str {
        "blockinfo"
    }
}

impl_vec!(BlockInfo);

/// Auxiliary structure used for blockchain syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockResponse {
    /// Response blocks.
    pub blocks: Vec<BlockInfo>,
}

impl net::Message for BlockResponse {
    fn name() -> &'static str {
        "blockresponse"
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockProposal {
    /// Leader public key
    pub public_key: PublicKey,
    /// Block signature
    pub signature: Signature,
    /// Leader address
    pub address: Address,
    /// Block data
    pub block: BlockInfo,
}

impl BlockProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        public_key: PublicKey,
        signature: Signature,
        address: Address,
        st: blake3::Hash,
        e: u64,
        sl: u64,
        txs: Vec<Transaction>,
        metadata: Metadata,
        sm: StreamletMetadata,
    ) -> Self {
        let block = BlockInfo::new(st, e, sl, txs, metadata, sm);
        Self { public_key, signature, address, block }
    }

    /// Produce proposal hash using `st`, `e`, `sl`, `txs`, and `metadata`.
    pub fn hash(&self) -> blake3::Hash {
        Self::to_proposal_hash(
            self.block.st,
            self.block.e,
            self.block.sl,
            &self.block.txs,
            &self.block.metadata,
        )
    }

    /// Generate a proposal hash using provided `st`, `e`, `sl`, `txs`, and `metadata`.
    pub fn to_proposal_hash(
        st: blake3::Hash,
        e: u64,
        sl: u64,
        transactions: &[Transaction],
        metadata: &Metadata,
    ) -> blake3::Hash {
        let mut txs = Vec::with_capacity(transactions.len());
        for tx in transactions {
            txs.push(blake3::hash(&serialize(tx)));
        }

        blake3::hash(&serialize(&Block::new(st, e, sl, txs, metadata.clone())))
    }
}

impl PartialEq for BlockProposal {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key &&
            self.signature == other.signature &&
            self.address == other.address &&
            self.block.st == other.block.st &&
            self.block.e == other.block.e &&
            self.block.sl == other.block.sl &&
            self.block.txs == other.block.txs &&
            self.block.metadata == other.block.metadata
    }
}

impl net::Message for BlockProposal {
    fn name() -> &'static str {
        "proposal"
    }
}

impl_vec!(BlockProposal);

impl From<BlockProposal> for BlockInfo {
    fn from(block: BlockProposal) -> BlockInfo {
        block.block
    }
}

/// This struct represents a sequence of block proposals.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ProposalChain {
    pub genesis_block: blake3::Hash,
    pub proposals: Vec<BlockProposal>,
}

impl ProposalChain {
    pub fn new(genesis_block: blake3::Hash, initial_proposal: BlockProposal) -> Self {
        Self { genesis_block, proposals: vec![initial_proposal] }
    }

    /// A proposal is considered valid when its parent hash is equal to the
    /// hash of the previous proposal and their slots are incremental,
    /// excluding the genesis block proposal.
    /// Additional validity rules can be applied.
    pub fn check_proposal(&self, proposal: &BlockProposal, previous: &BlockProposal) -> bool {
        if proposal.block.st == self.genesis_block {
            debug!("check_proposal(): Genesis block proposal provided.");
            return false
        }

        let prev_hash = previous.hash();
        if proposal.block.st != prev_hash || proposal.block.sl <= previous.block.sl {
            debug!("check_proposal(): Provided proposal is invalid.");
            return false
        }

        true
    }

    /// A proposals chain is considered valid when every proposal is valid,
    /// based on the `check_proposal` function.
    pub fn check_chain(&self) -> bool {
        for (index, proposal) in self.proposals[1..].iter().enumerate() {
            if !self.check_proposal(proposal, &self.proposals[index]) {
                return false
            }
        }

        true
    }

    /// Insertion of a valid proposal.
    pub fn add(&mut self, proposal: &BlockProposal) {
        if self.check_proposal(proposal, self.proposals.last().unwrap()) {
            self.proposals.push(proposal.clone());
        }
    }

    /// Proposals chain notarization check.
    pub fn notarized(&self) -> bool {
        for proposal in &self.proposals {
            if !proposal.block.sm.notarized {
                return false
            }
        }

        true
    }
}

impl_vec!(ProposalChain);
