/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::{anyhow, Result};
use darkfi::{rpc::jsonrpc::JsonRequest, tx::Transaction, wallet::walletdb::QueryType};
use darkfi_dao_contract::{
    dao_client::{
        DaoProposeNote, DAO_DAOS_COL_APPROVAL_RATIO_BASE, DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
        DAO_DAOS_COL_BULLA_BLIND, DAO_DAOS_COL_CALL_INDEX, DAO_DAOS_COL_DAO_ID,
        DAO_DAOS_COL_GOV_TOKEN_ID, DAO_DAOS_COL_LEAF_POSITION, DAO_DAOS_COL_NAME,
        DAO_DAOS_COL_PROPOSER_LIMIT, DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_SECRET,
        DAO_DAOS_COL_TX_HASH, DAO_DAOS_TABLE, DAO_PROPOSALS_COL_AMOUNT,
        DAO_PROPOSALS_COL_BULLA_BLIND, DAO_PROPOSALS_COL_CALL_INDEX, DAO_PROPOSALS_COL_DAO_ID,
        DAO_PROPOSALS_COL_LEAF_POSITION, DAO_PROPOSALS_COL_OUR_VOTE_ID,
        DAO_PROPOSALS_COL_PROPOSAL_ID, DAO_PROPOSALS_COL_RECV_PUBLIC,
        DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID, DAO_PROPOSALS_COL_SERIAL, DAO_PROPOSALS_COL_TX_HASH,
        DAO_PROPOSALS_TABLE, DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE,
        DAO_TREES_TABLE,
    },
    dao_model::{DaoBulla, DaoMintParams, DaoProposeParams},
    note::EncryptedNote2,
    DaoFunction,
};
use darkfi_sdk::{
    crypto::{
        poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey, TokenId, DAO_CONTRACT_ID,
    },
    incrementalmerkletree::{Position, Tree},
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};
use serde_json::json;

use super::Drk;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Parameters representing a DAO to be initialized
pub struct DaoParams {
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: pallas::Base,
}

#[derive(Debug, Clone)]
/// Parameters representing an intialized DAO, optionally deployed on-chain
pub struct Dao {
    /// Numeric identifier for the DAO
    pub id: u64,
    /// Named identifier for the DAO
    pub name: String,
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: pallas::Base,
    /// Leaf position of the DAO in the Merkle tree of DAOs
    pub leaf_position: Option<Position>,
    /// The transaction hash where the DAO was deployed
    pub tx_hash: Option<blake3::Hash>,
    /// The call index in the transaction where the DAO was deployed
    pub call_index: Option<u32>,
}

impl Dao {
    pub fn bulla(&self) -> DaoBulla {
        let (x, y) = PublicKey::from_secret(self.secret_key).xy();

        DaoBulla::from(poseidon_hash([
            pallas::Base::from(self.proposer_limit),
            pallas::Base::from(self.quorum),
            pallas::Base::from(self.approval_ratio_base),
            pallas::Base::from(self.approval_ratio_quot),
            self.gov_token_id.inner(),
            x,
            y,
            self.bulla_blind,
        ]))
    }
}

#[derive(Debug, Clone)]
/// Parameters representing an initialized DAO proposal, optionally deployed on-chain
pub struct DaoProposal {
    /// Numeric identifier for the proposal
    pub id: u64,
    /// The DAO bulla related to this proposal
    pub dao_bulla: DaoBulla,
    /// Recipient of this proposal's funds
    pub recipient: PublicKey,
    /// Amount of this proposal
    pub amount: u64,
    /// Serial of this proposal
    pub serial: pallas::Base,
    /// Token ID to be sent
    pub token_id: TokenId,
    /// Proposal's bulla blind
    pub bulla_blind: pallas::Base,
    /// Leaf position of this proposal in the Merkle tree of proposals
    pub leaf_position: Option<Position>,
    /// Transaction hash where this proposal was proposed
    pub tx_hash: Option<blake3::Hash>,
    /// call index in the transaction where this proposal was proposed
    pub call_index: Option<u32>,
    /// The vote ID we've voted on this proposal
    pub vote_id: Option<pallas::Base>,
}

impl DaoProposal {
    pub fn bulla(&self) -> pallas::Base {
        let (dest_x, dest_y) = self.recipient.xy();

        poseidon_hash([
            dest_x,
            dest_y,
            pallas::Base::from(self.amount),
            self.serial,
            self.token_id.inner(),
            self.dao_bulla.inner(),
            self.bulla_blind,
            self.bulla_blind,
        ])
    }
}

#[derive(Debug, Clone)]
/// Parameters representing a vote we've made on a DAO proposal
pub struct DaoVote {
    /// Numeric identifier for the vote
    pub id: u64,
    /// Numeric identifier for the proposal related to this vote
    pub proposal_id: u64,
    /// The vote
    pub vote_option: bool,
    /// Transaction hash where this vote was casted
    pub tx_hash: Option<blake3::Hash>,
    /// call index in the transaction where this vote was casted
    pub call_index: Option<u32>,
}

impl Drk {
    /// Initialize wallet with tables for the DAO contract
    pub async fn initialize_dao(&self) -> Result<()> {
        let wallet_schema = include_str!("../../../src/contract/dao/wallet.sql");

        // We perform a request to darkfid with the schema to initialize
        // the necessary tables in the wallet.
        let req = JsonRequest::new("wallet.exec_sql", json!([wallet_schema]));
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully initialized wallet schema for the DAO contract");
        } else {
            eprintln!("[initialize_dao] Got unexpected reply from darkfid: {}", rep);
        }

        // Check if we have to initialize the Merkle trees.
        // We check if one exists, but we actually create two. This should be written
        // a bit better and safer.
        let mut tree_needs_init = false;
        let query = format!("SELECT {} FROM {}", DAO_TREES_COL_DAOS_TREE, DAO_TREES_TABLE);
        let params = json!([query, QueryType::Blob as u8, DAO_TREES_COL_DAOS_TREE]);
        let req = JsonRequest::new("wallet.query_row_single", params);

        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        // TODO: The RPC needs a better variant for errors so detailed inspection
        //       can be done with error codes and all that.
        if (self.rpc_client.request(req).await).is_err() {
            tree_needs_init = true;
        }

        if tree_needs_init {
            eprintln!("Initializing DAO Merkle trees");
            let tree = MerkleTree::new(100);
            self.put_dao_trees(&tree, &tree).await?;
            eprintln!("Successfully initialized Merkle trees for the DAO contract");
        }

        Ok(())
    }

    /// Replace the DAO Merkle trees in the wallet.
    pub async fn put_dao_trees(
        &self,
        daos_tree: &MerkleTree,
        proposals_tree: &MerkleTree,
    ) -> Result<()> {
        let query = format!(
            "DELETE FROM {}; INSERT INTO {} ({}, {}) VALUES (?1, ?2);",
            DAO_TREES_TABLE, DAO_TREES_TABLE, DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            serialize(daos_tree),
            QueryType::Blob as u8,
            serialize(proposals_tree),
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Fetch DAO Merkle trees from the wallet
    pub async fn get_dao_trees(&self) -> Result<(MerkleTree, MerkleTree)> {
        let query = format!("SELECT * FROM {}", DAO_TREES_TABLE);

        let params = json!([
            query,
            QueryType::Blob as u8,
            DAO_TREES_COL_DAOS_TREE,
            QueryType::Blob as u8,
            DAO_TREES_COL_PROPOSALS_TREE,
        ]);

        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let daos_tree_bytes: Vec<u8> = serde_json::from_value(rep[0].clone())?;
        let daos_tree = deserialize(&daos_tree_bytes)?;

        let proposals_tree_bytes: Vec<u8> = serde_json::from_value(rep[1].clone())?;
        let proposals_tree = deserialize(&proposals_tree_bytes)?;

        Ok((daos_tree, proposals_tree))
    }

    /// Reset the DAO Merkle trees in the wallet
    pub async fn reset_dao_trees(&self) -> Result<()> {
        eprintln!("Resetting DAO Merkle trees");
        let tree = MerkleTree::new(100);
        self.put_dao_trees(&tree, &tree).await?;
        eprintln!("Successfully reset DAO Merkle trees");

        Ok(())
    }

    /// Import given DAO params into the wallet with a given name.
    pub async fn import_dao(&self, dao_name: String, dao_params: DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO with the given name before.
        let daos = self.get_daos().await?;
        if daos.iter().find(|x| x.name == dao_name).is_some() {
            return Err(anyhow!("This DAO has already been imported"))
        }

        eprintln!("Importing \"{}\" DAO into the wallet", dao_name);

        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            DAO_DAOS_TABLE,
            DAO_DAOS_COL_NAME,
            DAO_DAOS_COL_PROPOSER_LIMIT,
            DAO_DAOS_COL_QUORUM,
            DAO_DAOS_COL_APPROVAL_RATIO_BASE,
            DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
            DAO_DAOS_COL_GOV_TOKEN_ID,
            DAO_DAOS_COL_SECRET,
            DAO_DAOS_COL_BULLA_BLIND,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            serialize(&dao_name),
            QueryType::Integer as u8,
            dao_params.proposer_limit,
            QueryType::Integer as u8,
            dao_params.quorum,
            QueryType::Integer as u8,
            dao_params.approval_ratio_base,
            QueryType::Integer as u8,
            dao_params.approval_ratio_quot,
            QueryType::Blob as u8,
            serialize(&dao_params.gov_token_id),
            QueryType::Blob as u8,
            serialize(&dao_params.secret_key),
            QueryType::Blob as u8,
            serialize(&dao_params.bulla_blind),
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;
        eprintln!("DAO imported successfully");

        Ok(())
    }

    /// List DAO(s) imported in the wallet. If an ID is given, just print the
    /// metadata for that specific one, if found.
    pub async fn dao_list(&self, dao_id: Option<u64>) -> Result<()> {
        if dao_id.is_some() {
            return self.dao_list_single(dao_id.unwrap()).await
        }

        let daos = self.get_daos().await?;
        for dao in daos {
            println!("[{}] {}", dao.id, dao.name);
        }

        Ok(())
    }

    async fn dao_list_single(&self, dao_id: u64) -> Result<()> {
        let dao = self.get_dao_by_id(dao_id).await?;

        println!("DAO Parameters:");
        println!("Name: {}", dao.name);
        println!("Proposer limit: {}", dao.proposer_limit);
        println!("Quorum: {}", dao.quorum);
        println!(
            "Approval ratio: {}",
            dao.approval_ratio_base as f64 / dao.approval_ratio_quot as f64
        );
        println!("Governance token ID: {}", dao.gov_token_id);
        println!("Secret key: {}", dao.secret_key);
        println!("Bulla blind: {:?}", dao.bulla_blind);
        println!("Leaf position: {:?}", dao.leaf_position);
        println!("Tx hash: {:?}", dao.tx_hash);
        println!("Call idx: {:?}", dao.call_index);

        Ok(())
    }

    /// Fetch a DAO given a numeric ID
    pub async fn get_dao_by_id(&self, dao_id: u64) -> Result<Dao> {
        let daos = self.get_daos().await?;

        let Some(dao) = daos.iter().find(|x| x.id == dao_id) else {
            return Err(anyhow!("DAO not found in wallet"))
        };

        Ok(dao.clone())
    }

    /// Fetch all known DAOs from the wallet.
    pub async fn get_daos(&self) -> Result<Vec<Dao>> {
        let query = format!("SELECT * FROM {}", DAO_DAOS_TABLE);

        let params = json!([
            query,
            QueryType::Integer as u8,
            DAO_DAOS_COL_DAO_ID,
            QueryType::Blob as u8,
            DAO_DAOS_COL_NAME,
            QueryType::Integer as u8,
            DAO_DAOS_COL_PROPOSER_LIMIT,
            QueryType::Integer as u8,
            DAO_DAOS_COL_QUORUM,
            QueryType::Integer as u8,
            DAO_DAOS_COL_APPROVAL_RATIO_BASE,
            QueryType::Integer as u8,
            DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
            QueryType::Blob as u8,
            DAO_DAOS_COL_GOV_TOKEN_ID,
            QueryType::Blob as u8,
            DAO_DAOS_COL_SECRET,
            QueryType::Blob as u8,
            DAO_DAOS_COL_BULLA_BLIND,
            QueryType::OptionBlob as u8,
            DAO_DAOS_COL_LEAF_POSITION,
            QueryType::OptionBlob as u8,
            DAO_DAOS_COL_TX_HASH,
            QueryType::OptionInteger as u8,
            DAO_DAOS_COL_CALL_INDEX,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[get_daos] Unexpected response from darkfid: {}", rep));
        };

        let mut daos = Vec::with_capacity(rows.len());

        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("[get_daos] Unexpected response from darkfid: {}", rep));
            };

            let id: u64 = serde_json::from_value(row[0].clone())?;

            let name_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
            let name = deserialize(&name_bytes)?;

            let proposer_limit = serde_json::from_value(row[2].clone())?;
            let quorum = serde_json::from_value(row[3].clone())?;
            let approval_ratio_base = serde_json::from_value(row[4].clone())?;
            let approval_ratio_quot = serde_json::from_value(row[5].clone())?;

            let gov_token_bytes: Vec<u8> = serde_json::from_value(row[6].clone())?;
            let gov_token_id = deserialize(&gov_token_bytes)?;

            let secret_bytes: Vec<u8> = serde_json::from_value(row[7].clone())?;
            let secret_key = deserialize(&secret_bytes)?;

            let bulla_blind_bytes: Vec<u8> = serde_json::from_value(row[8].clone())?;
            let bulla_blind = deserialize(&bulla_blind_bytes)?;

            let leaf_position_bytes: Vec<u8> = serde_json::from_value(row[9].clone())?;
            let tx_hash_bytes: Vec<u8> = serde_json::from_value(row[10].clone())?;
            let call_index = serde_json::from_value(row[11].clone())?;

            let leaf_position = if leaf_position_bytes.is_empty() {
                None
            } else {
                Some(deserialize(&leaf_position_bytes)?)
            };

            let tx_hash =
                if tx_hash_bytes.is_empty() { None } else { Some(deserialize(&tx_hash_bytes)?) };

            let dao = Dao {
                id,
                name,
                proposer_limit,
                quorum,
                approval_ratio_base,
                approval_ratio_quot,
                gov_token_id,
                secret_key,
                bulla_blind,
                leaf_position,
                tx_hash,
                call_index,
            };

            daos.push(dao);
        }

        // Here we sort the vec by ID. The SQL SELECT statement does not guarantee
        // this, so just do it here.
        daos.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(daos)
    }

    /// Fetch all known DAO proposals from the wallet given a DAO ID
    pub async fn get_dao_proposals(&self, dao_id: u64) -> Result<Vec<DaoProposal>> {
        let daos = self.get_daos().await?;
        let Some(dao) = daos.get(dao_id as usize - 1) else {
            return Err(anyhow!("DAO with ID {} not found in wallet", dao_id))
        };

        let query = format!(
            "SELECT * FROM {} WHERE {} = {}",
            DAO_PROPOSALS_TABLE, DAO_PROPOSALS_COL_DAO_ID, dao_id
        );

        let params = json!([
            query,
            QueryType::Integer as u8,
            DAO_PROPOSALS_COL_PROPOSAL_ID,
            QueryType::Integer as u8,
            DAO_PROPOSALS_COL_DAO_ID,
            QueryType::Blob as u8,
            DAO_PROPOSALS_COL_RECV_PUBLIC,
            QueryType::Blob as u8,
            DAO_PROPOSALS_COL_AMOUNT,
            QueryType::Blob as u8,
            DAO_PROPOSALS_COL_SERIAL,
            QueryType::Blob as u8,
            DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID,
            QueryType::Blob as u8,
            DAO_PROPOSALS_COL_BULLA_BLIND,
            QueryType::OptionBlob as u8,
            DAO_PROPOSALS_COL_LEAF_POSITION,
            QueryType::OptionBlob as u8,
            DAO_PROPOSALS_COL_TX_HASH,
            QueryType::OptionInteger as u8,
            DAO_PROPOSALS_COL_CALL_INDEX,
            QueryType::OptionBlob as u8,
            DAO_PROPOSALS_COL_OUR_VOTE_ID,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[get_proposals] Unexpected response from darkfid: {}", rep));
        };

        let mut proposals = Vec::with_capacity(rows.len());

        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("[get_proposals] Unexpected response from darkfid: {}", rep));
            };

            let id: u64 = serde_json::from_value(row[0].clone())?;

            let dao_id: u64 = serde_json::from_value(row[1].clone())?;
            assert!(dao_id == dao.id);
            let dao_bulla = dao.bulla();

            let recipient_bytes: Vec<u8> = serde_json::from_value(row[2].clone())?;
            let recipient = deserialize(&recipient_bytes)?;

            let amount_bytes: Vec<u8> = serde_json::from_value(row[3].clone())?;
            let amount = deserialize(&amount_bytes)?;

            let serial_bytes: Vec<u8> = serde_json::from_value(row[4].clone())?;
            let serial = deserialize(&serial_bytes)?;

            let token_id_bytes: Vec<u8> = serde_json::from_value(row[5].clone())?;
            let token_id = deserialize(&token_id_bytes)?;

            let bulla_blind_bytes: Vec<u8> = serde_json::from_value(row[6].clone())?;
            let bulla_blind = deserialize(&bulla_blind_bytes)?;

            let leaf_position_bytes: Vec<u8> = serde_json::from_value(row[7].clone())?;
            let tx_hash_bytes: Vec<u8> = serde_json::from_value(row[8].clone())?;
            let call_index = serde_json::from_value(row[9].clone())?;
            let vote_id_bytes: Vec<u8> = serde_json::from_value(row[10].clone())?;

            let leaf_position = if leaf_position_bytes.is_empty() {
                None
            } else {
                Some(deserialize(&leaf_position_bytes)?)
            };

            let tx_hash =
                if tx_hash_bytes.is_empty() { None } else { Some(deserialize(&tx_hash_bytes)?) };

            let vote_id =
                if vote_id_bytes.is_empty() { None } else { Some(deserialize(&vote_id_bytes)?) };

            let proposal = DaoProposal {
                id,
                dao_bulla,
                recipient,
                amount,
                serial,
                token_id,
                bulla_blind,
                leaf_position,
                tx_hash,
                call_index,
                vote_id,
            };

            proposals.push(proposal);
        }

        // Here we sort the vec by ID. The SQL SELECT statement does not guarantee
        // this, so just do it here.
        proposals.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(proposals)
    }

    // Fetch all known DAO proposal votes from the wallet given a proposal ID
    //pub async fn get_dao_proposal_votes(&self, _proposal_id: u64) -> Result<Vec<Vote>> {
    //todo!()
    //}

    /// Append data related to DAO contract transactions into the wallet database.
    /// Optionally, if `confirm` is true, also append the data in the Merkle trees, etc.
    pub async fn apply_tx_dao_data(&self, tx: &Transaction, confirm: bool) -> Result<()> {
        let cid = *DAO_CONTRACT_ID;
        let mut daos = self.get_daos().await?;
        let (mut daos_tree, mut proposals_tree) = self.get_dao_trees().await?;

        // DAOs that have been minted
        let mut new_dao_bullas: Vec<(DaoBulla, Option<blake3::Hash>, u32)> = vec![];
        // DAO proposals that have been minted
        let mut new_dao_proposals: Vec<(DaoProposeParams, Option<blake3::Hash>, u32)> = vec![];
        let mut our_proposals: Vec<DaoProposal> = vec![];

        // Run through the transaction and see what we got:
        for (i, call) in tx.calls.iter().enumerate() {
            if call.contract_id == cid && call.data[0] == DaoFunction::Mint as u8 {
                eprintln!("Found Dao::Mint in call {}", i);
                let params: DaoMintParams = deserialize(&call.data[1..])?;
                let tx_hash = if confirm { Some(blake3::hash(&serialize(tx))) } else { None };
                new_dao_bullas.push((params.dao_bulla, tx_hash, i as u32));
                continue
            }

            if call.contract_id == cid && call.data[0] == DaoFunction::Propose as u8 {
                eprintln!("Found Dao::Propose in call {}", i);
                let params: DaoProposeParams = deserialize(&call.data[1..])?;
                let tx_hash = if confirm { Some(blake3::hash(&serialize(tx))) } else { None };
                new_dao_proposals.push((params, tx_hash, i as u32));
                continue
            }

            if call.contract_id == cid && call.data[0] == DaoFunction::Vote as u8 {
                eprintln!("[UNIMPLEMENTED] Found Dao::Vote in call {}", i);
                continue
            }

            if call.contract_id == cid && call.data[0] == DaoFunction::Exec as u8 {
                eprintln!("[UNIMPLEMENTED] Found Dao::Exec in call {}", i);
                continue
            }
        }

        // This code should only be executed when finalized blocks are being scanned.
        // Here we write the tx metadata, and actually do Merkle tree appends so we
        // have to make sure it's the same for everyone.
        if confirm {
            for new_bulla in new_dao_bullas {
                daos_tree.append(&MerkleNode::from(new_bulla.0.inner()));
                for dao in daos.iter_mut() {
                    if dao.bulla() == new_bulla.0 {
                        eprintln!(
                            "Found minted DAO {:?}, noting down for wallet update",
                            new_bulla.0
                        );
                        // We have this DAO imported in our wallet. Add the metadata:
                        dao.leaf_position = daos_tree.witness();
                        dao.tx_hash = new_bulla.1;
                        dao.call_index = Some(new_bulla.2);
                    }
                }
            }

            for proposal in new_dao_proposals {
                proposals_tree.append(&MerkleNode::from(proposal.0.proposal_bulla));
                // FIXME: EncryptedNote2 should perhaps be something generic?
                let enc_note = EncryptedNote2 {
                    ciphertext: proposal.0.ciphertext,
                    ephem_public: proposal.0.ephem_public,
                };

                // If we're able to decrypt this note, that's the way to link it
                // to a specific DAO.
                for dao in &daos {
                    if let Ok(note) = enc_note.decrypt::<DaoProposeNote>(&dao.secret_key) {
                        // We managed to decrypt it. Let's place this in a proper
                        // DaoProposal object. We assume we can just increment the
                        // ID by looking at how many proposals we already have.
                        // We also assume we don't mantain duplicate DAOs in the
                        // wallet.
                        eprintln!("Managed to decrypt DAO proposal note");
                        let daos_proposals = self.get_dao_proposals(dao.id).await?;
                        let our_prop = DaoProposal {
                            // This ID stuff is flaky.
                            id: daos_proposals.len() as u64 + our_proposals.len() as u64 + 1,
                            dao_bulla: dao.bulla(),
                            recipient: note.proposal.dest,
                            amount: note.proposal.amount,
                            serial: note.proposal.serial,
                            token_id: note.proposal.token_id,
                            bulla_blind: note.proposal.blind,
                            leaf_position: proposals_tree.witness(),
                            tx_hash: proposal.1,
                            call_index: Some(proposal.2),
                            vote_id: None,
                        };

                        our_proposals.push(our_prop);
                        break
                    }
                }
            }
        }

        if confirm {
            self.confirm_daos(&daos).await?;
            self.put_dao_proposals(&our_proposals).await?;
        }

        Ok(())
    }

    /// Confirm already imported DAO metadata into the wallet.
    /// Here we just write the leaf position, tx hash, and call index.
    /// Panics if the fields are None.
    pub async fn confirm_daos(&self, daos: &[Dao]) -> Result<()> {
        for dao in daos {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = ?4;",
                DAO_DAOS_TABLE,
                DAO_DAOS_COL_LEAF_POSITION,
                DAO_DAOS_COL_TX_HASH,
                DAO_DAOS_COL_CALL_INDEX,
                DAO_DAOS_COL_DAO_ID,
            );

            let params = json!([
                query,
                QueryType::Blob as u8,
                serialize(&dao.leaf_position.unwrap()),
                QueryType::Blob as u8,
                serialize(&dao.tx_hash.unwrap()),
                QueryType::Integer as u8,
                dao.call_index.unwrap(),
            ]);

            let req = JsonRequest::new("wallet.exec_sql", params);
            let _ = self.rpc_client.request(req).await?;
        }

        Ok(())
    }

    /// Import given DAO proposals into the wallet
    pub async fn put_dao_proposals(&self, proposals: &[DaoProposal]) -> Result<()> {
        let daos = self.get_daos().await?;

        for proposal in proposals {
            let Some(dao) = daos.iter().find(|x| x.bulla() == proposal.dao_bulla) else {
                return Err(anyhow!("[put_dao_proposals] Couldn't find respective DAO"))
            };

            let query = format!(
                "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
                DAO_PROPOSALS_TABLE,
                DAO_PROPOSALS_COL_DAO_ID,
                DAO_PROPOSALS_COL_RECV_PUBLIC,
                DAO_PROPOSALS_COL_AMOUNT,
                DAO_PROPOSALS_COL_SERIAL,
                DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID,
                DAO_PROPOSALS_COL_BULLA_BLIND,
                DAO_PROPOSALS_COL_LEAF_POSITION,
                DAO_PROPOSALS_COL_TX_HASH,
                DAO_PROPOSALS_COL_CALL_INDEX,
            );

            let params = json!([
                query,
                QueryType::Integer as u8,
                dao.id,
                QueryType::Blob as u8,
                serialize(&proposal.recipient),
                QueryType::Blob as u8,
                serialize(&proposal.amount),
                QueryType::Blob as u8,
                serialize(&proposal.serial),
                QueryType::Blob as u8,
                serialize(&proposal.token_id),
                QueryType::Blob as u8,
                serialize(&proposal.bulla_blind),
                QueryType::Blob as u8,
                serialize(&proposal.leaf_position.unwrap()),
                QueryType::Blob as u8,
                serialize(&proposal.tx_hash.unwrap()),
                QueryType::Integer as u8,
                proposal.call_index,
            ]);

            let req = JsonRequest::new("wallet.exec_sql", params);
            let _ = self.rpc_client.request(req).await?;
        }

        Ok(())
    }
}
