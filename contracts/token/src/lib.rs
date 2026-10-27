//! CLAW: NEP-141 fungible token with NEP-145 storage management.
//!
//! The full supply is minted once in `new` to the distribution account. There is no
//! mint, upgrade or owner method; holders can burn their own tokens (used by buybacks).
//!
//! Trading fee: every transfer to or from a listed DEX contract (a buy or a sell) pays
//! `FEE_BPS` (2%) to the fee router, which converts it to NEAR and routes it (80% to
//! House of Stake by default, 20% to the dev wallet). The rate is fixed in code. Only the
//! Governor (Tier 1) can change which DEX contracts are listed. Transfers to or from the
//! fee router itself are exempt, so its own swaps are not taxed.

use near_contract_standards::fungible_token::core::FungibleTokenCore;
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::resolver::FungibleTokenResolver;
use near_contract_standards::fungible_token::FungibleToken;
use near_contract_standards::fungible_token::events::{FtBurn, FtMint};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, PromiseOrValue,
};

/// 2% on buys and sells. Fixed: no method can change it.
pub const FEE_BPS: u128 = 200;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
    FeePools,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct FeeConfig {
    pub fee_bps: u16,
    pub fee_collector: AccountId,
    pub governor_id: AccountId,
    pub fee_pools: Vec<AccountId>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    governor_id: AccountId,
    fee_collector: AccountId,
    fee_pools: IterableSet<AccountId>,
}

#[near]
impl Contract {
    /// Mint the fixed supply to `distribution_id` and store metadata. Called once at deploy.
    #[init]
    pub fn new(
        distribution_id: AccountId,
        total_supply: U128,
        metadata: FungibleTokenMetadata,
        governor_id: AccountId,
        fee_collector: AccountId,
        fee_pools: Vec<AccountId>,
    ) -> Self {
        require!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();
        require!(metadata.spec == FT_METADATA_SPEC, "Bad metadata spec");
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            governor_id,
            fee_collector: fee_collector.clone(),
            fee_pools: IterableSet::new(StorageKey::FeePools),
        };
        for p in fee_pools {
            this.fee_pools.insert(p);
        }
        this.token.internal_register_account(&distribution_id);
        if fee_collector != distribution_id {
            this.token.internal_register_account(&fee_collector);
        }
        this.token.internal_deposit(&distribution_id, total_supply.into());
        FtMint {
            owner_id: &distribution_id,
            amount: total_supply,
            memo: Some("fixed supply, no further mint"),
        }
        .emit();
        this
    }

    /// Burn your own tokens. The fee router burns buyback proceeds with this.
    #[payable]
    pub fn ft_burn(&mut self, amount: U128) {
        near_sdk::assert_one_yocto();
        let owner = env::predecessor_account_id();
        self.token.internal_withdraw(&owner, amount.0);
        FtBurn { owner_id: &owner, amount, memo: Some("burn") }.emit();
    }

    /// Tier 1: list or delist DEX contracts whose trades pay the fee.
    pub fn set_fee_pools(&mut self, add: Vec<AccountId>, remove: Vec<AccountId>) {
        require!(env::predecessor_account_id() == self.governor_id, "Only the Governor can call this method");
        for p in remove {
            self.fee_pools.remove(&p);
        }
        for p in add {
            require!(p != self.fee_collector, "Fee router cannot be a pool");
            self.fee_pools.insert(p);
        }
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({
                "standard": "clawdao", "version": "1.0.0", "event": "fee_pools_set",
                "data": [{ "pools": self.fee_pools.iter().collect::<Vec<_>>() }],
            })
        ));
    }

    pub fn fee_config(&self) -> FeeConfig {
        FeeConfig {
            fee_bps: FEE_BPS as u16,
            fee_collector: self.fee_collector.clone(),
            governor_id: self.governor_id.clone(),
            fee_pools: self.fee_pools.iter().cloned().collect(),
        }
    }

    /// What `receiver` gets if `sender` transfers `amount` (after any trading fee).
    pub fn net_of_fee(&self, sender_id: AccountId, receiver_id: AccountId, amount: U128) -> U128 {
        U128(amount.0 - self.fee_for(&sender_id, &receiver_id, amount.0))
    }
}

impl Contract {
    fn fee_for(&self, sender: &AccountId, receiver: &AccountId, amount: u128) -> u128 {
        let exempt = *sender == self.fee_collector || *receiver == self.fee_collector;
        let trade = self.fee_pools.contains(sender) || self.fee_pools.contains(receiver);
        if trade && !exempt {
            amount * FEE_BPS / 10_000
        } else {
            0
        }
    }

    /// Moves the fee to the collector and returns what is left to transfer.
    fn take_fee(&mut self, receiver: &AccountId, amount: u128) -> u128 {
        let sender = env::predecessor_account_id();
        let fee = self.fee_for(&sender, receiver, amount);
        if fee > 0 {
            let collector = self.fee_collector.clone();
            self.token.internal_transfer(&sender, &collector, fee, Some("clawdao trading fee".into()));
        }
        let net = amount - fee;
        require!(net > 0, "Amount too small after fee");
        net
    }
}

#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        near_sdk::assert_one_yocto();
        let net = self.take_fee(&receiver_id, amount.0);
        self.token.ft_transfer(receiver_id, U128(net), memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        near_sdk::assert_one_yocto();
        // The receiver is told the net amount, so DEX accounting matches what arrived.
        let net = self.take_fee(&receiver_id, amount.0);
        self.token.ft_transfer_call(receiver_id, U128(net), memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used, burned) = self.token.internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned > 0 {
            FtBurn {
                owner_id: &sender_id,
                amount: U128(burned),
                memo: Some("refund to closed account"),
            }
            .emit();
        }
        used.into()
    }
}

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        // Never allow force-unregister with a balance: it would burn tokens.
        require!(force != Some(true), "force unregister is disabled");
        self.token.internal_storage_unregister(force).is_some()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const SUPPLY: u128 = 1_000_000_000 * 10u128.pow(24);

    fn dex() -> AccountId {
        "v2.ref-finance.near".parse().unwrap()
    }
    fn router() -> AccountId {
        "fees.claw.near".parse().unwrap()
    }
    fn new(dist: AccountId) -> Contract {
        Contract::new(dist, U128(SUPPLY), meta(), accounts(5), router(), vec![dex()])
    }
    fn register(ctx: &mut VMContextBuilder, c: &mut Contract, who: AccountId) {
        testing_env!(ctx.attached_deposit(c.storage_balance_bounds().min).predecessor_account_id(who.clone()).build());
        c.storage_deposit(Some(who), None);
    }
    fn send(ctx: &mut VMContextBuilder, c: &mut Contract, from: AccountId, to: AccountId, amount: u128) {
        testing_env!(ctx.attached_deposit(NearToken::from_yoctonear(1)).predecessor_account_id(from).build());
        c.ft_transfer(to, U128(amount), None);
    }

    #[test]
    fn sell_and_buy_pay_two_percent() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        let mut c = new(accounts(1));
        register(&mut ctx, &mut c, dex());
        register(&mut ctx, &mut c, accounts(2));
        // Sell: holder -> DEX.
        send(&mut ctx, &mut c, accounts(1), dex(), 10_000);
        assert_eq!(c.ft_balance_of(dex()).0, 9_800);
        assert_eq!(c.ft_balance_of(router()).0, 200);
        // Buy: DEX -> holder.
        send(&mut ctx, &mut c, dex(), accounts(2), 5_000);
        assert_eq!(c.ft_balance_of(accounts(2)).0, 4_900);
        assert_eq!(c.ft_balance_of(router()).0, 300);
        assert_eq!(c.ft_total_supply().0, SUPPLY);
    }

    #[test]
    fn wallet_transfers_and_router_swaps_are_free() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        let mut c = new(accounts(1));
        register(&mut ctx, &mut c, dex());
        register(&mut ctx, &mut c, accounts(2));
        send(&mut ctx, &mut c, accounts(1), accounts(2), 1_000);
        assert_eq!(c.ft_balance_of(accounts(2)).0, 1_000);
        send(&mut ctx, &mut c, accounts(1), router(), 1_000);
        send(&mut ctx, &mut c, router(), dex(), 1_000);
        assert_eq!(c.ft_balance_of(dex()).0, 1_000, "router sells are exempt");
    }

    #[test]
    fn burn_reduces_supply() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).attached_deposit(NearToken::from_yoctonear(1)).build());
        let mut c = new(accounts(1));
        c.ft_burn(U128(7));
        assert_eq!(c.ft_total_supply().0, SUPPLY - 7);
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn pools_governor_only() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        let mut c = new(accounts(1));
        c.set_fee_pools(vec![accounts(3)], vec![]);
    }

    #[test]
    fn governor_lists_new_dex() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        let mut c = new(accounts(1));
        testing_env!(ctx.predecessor_account_id(accounts(5)).build());
        c.set_fee_pools(vec![accounts(3)], vec![dex()]);
        assert_eq!(c.fee_config().fee_pools, vec![accounts(3)]);
        assert_eq!(c.net_of_fee(accounts(1), accounts(3), U128(100)).0, 98);
    }

    fn meta() -> FungibleTokenMetadata {
        FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "IronClaw Coin".into(),
            symbol: "CLAW".into(),
            icon: None,
            reference: None,
            reference_hash: None,
            decimals: 24,
        }
    }

    #[test]
    fn fixed_supply_minted_to_distribution() {
        testing_env!(VMContextBuilder::new().predecessor_account_id(accounts(0)).build());
        let c = new(accounts(1));
        assert_eq!(c.ft_total_supply().0, SUPPLY);
        assert_eq!(c.ft_balance_of(accounts(1)).0, SUPPLY);
        assert_eq!(c.ft_metadata().symbol, "CLAW");
    }

    #[test]
    fn transfer_moves_balance() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        let mut c = new(accounts(1));
        testing_env!(ctx
            .attached_deposit(c.storage_balance_bounds().min)
            .predecessor_account_id(accounts(2))
            .build());
        c.storage_deposit(None, None);
        testing_env!(ctx
            .attached_deposit(NearToken::from_yoctonear(1))
            .predecessor_account_id(accounts(1))
            .build());
        c.ft_transfer(accounts(2), U128(5), None);
        assert_eq!(c.ft_balance_of(accounts(2)).0, 5);
        assert_eq!(c.ft_balance_of(accounts(1)).0, SUPPLY - 5);
    }
}
