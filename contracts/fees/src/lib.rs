//! Fee router: turns the 2% CLAW trading fee into NEAR and routes it.
//!
//! Every step is permissionless, so a keeper (or anyone) drives it:
//! 1. `swap_fees`: sell up to `max_swap_per_call` collected CLAW for wNEAR on Rhea/Ref,
//!    with a minimum output from an on-chain quote less `max_slippage_bps`.
//! 2. `unwrap`: wNEAR → NEAR, added to `pending_near`.
//! 3. `distribute`: `dev_bps` (20%) to the dev wallet; the rest by `mode`:
//!    StakeHoS → agent contract locks it in House of Stake (default);
//!    HolderRewards → paid to locked-CLAW holders through the ve contract;
//!    Buyback → queued for `buyback`.
//! 4. `buyback`: NEAR → wNEAR → CLAW on Rhea/Ref, then burned.
//!
//! The mode is StakeHoS until CLAW's market cap has been at or above `mcap_threshold_usd`
//! ($100k) on 3 consecutive UTC days, measured by `observe_market_cap` from Rhea/Ref pools.
//! After that, CLAW holders choose the mode by a Tier 2 `SetFeeMode` vote. The dev wallet and
//! share change only by Tier 1 vote, and the share can never exceed 20%.

use clawdao_common::{assert_caller, emit_event, mul_div, DAY_NS, NEAR_ONE};
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{env, near, require, serde_json, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError};

const MAX_DEV_BPS: u16 = 2_000;
const UNLOCK_STREAK_DAYS: u8 = 3;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const GAS_VIEW: Gas = Gas::from_tgas(10);
const GAS_FT_TRANSFER_CALL: Gas = Gas::from_tgas(100);
const GAS_SIMPLE: Gas = Gas::from_tgas(10);
const GAS_ROUTE: Gas = Gas::from_tgas(80);
const GAS_CALLBACK: Gas = Gas::from_tgas(20);

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeeMode {
    StakeHoS,
    HolderRewards,
    Buyback,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct DexConfig {
    /// Rhea/Ref exchange (`v2.ref-finance.near`).
    pub dex_id: AccountId,
    /// wNEAR (`wrap.near`).
    pub wrap_id: AccountId,
    /// CLAW/wNEAR pool used for swaps and price.
    pub claw_pool_id: u64,
    /// wNEAR/USD-stablecoin pool used to price NEAR in USD.
    pub usd_pool_id: u64,
    pub usd_token_id: AccountId,
    pub usd_decimals: u8,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Limits {
    /// Most CLAW sold per `swap_fees` call (limits price impact and sandwich value).
    pub max_swap_per_call: U128,
    /// Most NEAR spent per `buyback` call.
    pub max_buyback_per_call: U128,
    /// Accepted slippage against the on-chain quote, at most 5%.
    pub max_slippage_bps: u16,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct FeeState {
    pub mode: FeeMode,
    pub mode_unlocked: bool,
    pub mcap_threshold_usd: U128,
    pub streak_days: u8,
    pub last_mcap_usd: U128,
    pub dev_wallet: AccountId,
    pub dev_bps: u16,
    pub pending_near: U128,
    pub buyback_near: U128,
    pub totals: Totals,
    pub dex: DexConfig,
    pub limits: Limits,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub near_from_fees: U128,
    pub to_dev: U128,
    pub to_hos: U128,
    pub to_holders: U128,
    pub to_buyback: U128,
    pub claw_burned: U128,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    token_id: AccountId,
    governor_id: AccountId,
    agent_id: AccountId,
    ve_id: AccountId,
    dex: DexConfig,
    limits: Limits,
    dev_wallet: AccountId,
    dev_bps: u16,
    mode: FeeMode,
    mode_unlocked: bool,
    mcap_threshold_usd: u128,
    streak_days: u8,
    last_observed_day: u64,
    last_mcap_usd: u128,
    pending_near: u128,
    buyback_near: u128,
    swapping: bool,
    buying_back: bool,
    totals: Totals,
    /// May set the pool ids once at launch (the CLAW pool only exists after the token
    /// is deployed). Cleared on use; afterwards only the Governor can change the DEX config.
    launcher: Option<AccountId>,
}

#[near]
impl Contract {
    #[init]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        token_id: AccountId,
        governor_id: AccountId,
        agent_id: AccountId,
        ve_id: AccountId,
        dex: DexConfig,
        limits: Limits,
        dev_wallet: AccountId,
        mcap_threshold_usd: U128,
    ) -> Self {
        require!(limits.max_slippage_bps <= 500, "Slippage above 5%");
        Self {
            token_id,
            governor_id,
            agent_id,
            ve_id,
            dex,
            limits,
            dev_wallet,
            dev_bps: MAX_DEV_BPS,
            mode: FeeMode::StakeHoS,
            mode_unlocked: false,
            mcap_threshold_usd: mcap_threshold_usd.0,
            streak_days: 0,
            last_observed_day: 0,
            last_mcap_usd: 0,
            pending_near: 0,
            buyback_near: 0,
            swapping: false,
            buying_back: false,
            totals: Totals::default(),
            launcher: Some(env::predecessor_account_id()),
        }
    }

    /// One-time, by the deploying account: set the pool ids once liquidity exists.
    pub fn launch_set_pools(&mut self, claw_pool_id: u64, usd_pool_id: u64) {
        require!(self.launcher.as_ref() == Some(&env::predecessor_account_id()), "Only the launcher, once");
        self.launcher = None;
        self.dex.claw_pool_id = claw_pool_id;
        self.dex.usd_pool_id = usd_pool_id;
        emit_event("fee_dex_set", &self.dex);
    }

    /// Tier 1: change the DEX, wNEAR or pools.
    pub fn set_dex(&mut self, dex: DexConfig) {
        assert_caller(&self.governor_id, "the Governor");
        emit_event("fee_dex_set", &dex);
        self.dex = dex;
    }

    /// Register this contract on wNEAR and the DEX. Anyone can pay for it.
    #[payable]
    pub fn setup(&mut self, wrap_storage: U128, dex_storage: U128) -> Promise {
        require!(env::attached_deposit().as_yoctonear() >= wrap_storage.0 + dex_storage.0, "Attach the storage deposits");
        let me = env::current_account_id();
        Promise::new(self.dex.wrap_id.clone())
            .function_call(
                "storage_deposit".to_string(),
                serde_json::to_vec(&serde_json::json!({ "account_id": me })).unwrap(),
                NearToken::from_yoctonear(wrap_storage.0),
                GAS_SIMPLE,
            )
            .and(Promise::new(self.dex.dex_id.clone()).function_call(
                "storage_deposit".to_string(),
                serde_json::to_vec(&serde_json::json!({ "account_id": me, "registration_only": false })).unwrap(),
                NearToken::from_yoctonear(dex_storage.0),
                GAS_SIMPLE,
            ))
    }

    // ---------------------------------------------------------------- 1. swap

    pub fn swap_fees(&mut self) -> Promise {
        require!(!self.swapping, "Swap in progress");
        self.swapping = true;
        self.view(&self.token_id, "ft_balance_of", serde_json::json!({ "account_id": env::current_account_id() }))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_fee_balance())
    }

    #[private]
    pub fn on_fee_balance(&mut self) -> Promise {
        let balance = read_u128(0).unwrap_or(0);
        let amount = balance.min(self.limits.max_swap_per_call.0);
        if amount == 0 {
            self.swapping = false;
            env::panic_str("No fees to swap");
        }
        self.quote(self.dex.claw_pool_id, &self.token_id.clone(), amount, &self.dex.wrap_id.clone())
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_fee_quote(U128(amount)))
    }

    #[private]
    pub fn on_fee_quote(&mut self, amount: U128) -> Promise {
        let Some(quote) = read_u128(0) else {
            self.swapping = false;
            env::panic_str("Quote failed");
        };
        let min_out = self.min_out(quote);
        let msg = swap_msg(self.dex.claw_pool_id, &self.token_id, &self.dex.wrap_id, amount.0, min_out);
        Promise::new(self.token_id.clone())
            .function_call(
                "ft_transfer_call".to_string(),
                serde_json::to_vec(&serde_json::json!({ "receiver_id": self.dex.dex_id, "amount": amount, "msg": msg })).unwrap(),
                ONE_YOCTO,
                GAS_FT_TRANSFER_CALL,
            )
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_fee_swapped(amount, U128(min_out)))
    }

    #[private]
    pub fn on_fee_swapped(&mut self, amount: U128, min_out: U128) {
        self.swapping = false;
        emit_event("fees_swapped", &serde_json::json!({ "claw_in": amount, "min_wnear_out": min_out }));
    }

    // ---------------------------------------------------------------- 2. unwrap

    pub fn unwrap(&mut self) -> Promise {
        self.view(&self.dex.wrap_id, "ft_balance_of", serde_json::json!({ "account_id": env::current_account_id() }))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_wnear_balance())
    }

    #[private]
    pub fn on_wnear_balance(&mut self) -> Promise {
        let amount = read_u128(0).unwrap_or(0);
        require!(amount > 0, "No wNEAR to unwrap");
        Promise::new(self.dex.wrap_id.clone())
            .function_call(
                "near_withdraw".to_string(),
                serde_json::to_vec(&serde_json::json!({ "amount": U128(amount) })).unwrap(),
                ONE_YOCTO,
                GAS_SIMPLE,
            )
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_unwrapped(U128(amount)))
    }

    #[private]
    pub fn on_unwrapped(&mut self, amount: U128) {
        if succeeded(0) {
            self.pending_near += amount.0;
            self.totals.near_from_fees.0 += amount.0;
            emit_event("fees_unwrapped", &serde_json::json!({ "near": amount }));
        }
    }

    // ---------------------------------------------------------------- 3. distribute

    pub fn distribute(&mut self) -> Promise {
        let amount = self.pending_near;
        require!(amount >= NEAR_ONE / 10, "Less than 0.1 NEAR pending");
        self.pending_near = 0;
        let dev = mul_div(amount, self.dev_bps as u128, 10_000);
        let rest = amount - dev;
        self.totals.to_dev.0 += dev;
        emit_event("fees_distributed", &serde_json::json!({
            "near": U128(amount), "to_dev": U128(dev), "to_mode": U128(rest), "mode": self.mode,
        }));
        let dev_transfer = Promise::new(self.dev_wallet.clone()).transfer(NearToken::from_yoctonear(dev));
        let route = match self.mode {
            FeeMode::StakeHoS => Promise::new(self.agent_id.clone()).function_call(
                "hos_lock_fees".to_string(),
                b"{}".to_vec(),
                NearToken::from_yoctonear(rest),
                GAS_ROUTE,
            ),
            FeeMode::HolderRewards => Promise::new(self.ve_id.clone()).function_call(
                "deposit_rewards".to_string(),
                b"{}".to_vec(),
                NearToken::from_yoctonear(rest),
                GAS_ROUTE,
            ),
            FeeMode::Buyback => {
                self.buyback_near += rest;
                self.totals.to_buyback.0 += rest;
                return dev_transfer;
            }
        };
        dev_transfer.and(route).then(
            Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_routed(U128(rest), self.mode),
        )
    }

    #[private]
    pub fn on_routed(&mut self, amount: U128, mode: FeeMode) {
        if succeeded(1) {
            match mode {
                FeeMode::StakeHoS => self.totals.to_hos.0 += amount.0,
                FeeMode::HolderRewards => self.totals.to_holders.0 += amount.0,
                FeeMode::Buyback => {}
            }
        } else {
            // The attached NEAR came back; retry on the next distribute.
            self.pending_near += amount.0;
        }
    }

    // ---------------------------------------------------------------- 4. buyback

    /// Anyone can add NEAR to the buyback queue (the agent contract does, for allocations).
    #[payable]
    pub fn deposit_buyback(&mut self) {
        let amount = env::attached_deposit().as_yoctonear();
        self.buyback_near += amount;
        self.totals.to_buyback.0 += amount;
        emit_event("buyback_deposited", &serde_json::json!({ "near": U128(amount) }));
    }

    pub fn buyback(&mut self) -> Promise {
        require!(!self.buying_back, "Buyback in progress");
        let amount = self.buyback_near.min(self.limits.max_buyback_per_call.0);
        require!(amount > 0, "Nothing to buy back");
        self.buying_back = true;
        self.buyback_near -= amount;
        let me = env::current_account_id();
        Promise::new(self.dex.wrap_id.clone())
            .function_call("near_deposit".to_string(), b"{}".to_vec(), NearToken::from_yoctonear(amount), GAS_SIMPLE)
            .then(
                self.quote(self.dex.claw_pool_id, &self.dex.wrap_id.clone(), amount, &self.token_id.clone())
                    .and(self.view(&self.token_id, "ft_balance_of", serde_json::json!({ "account_id": me }))),
            )
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_buyback_quote(U128(amount)))
    }

    #[private]
    pub fn on_buyback_quote(&mut self, amount: U128) -> Promise {
        let (Some(quote), Some(before)) = (read_u128(0), read_u128(1)) else {
            self.buying_back = false;
            self.buyback_near += amount.0; // wNEAR is held; unwrap returns it to pending later
            env::panic_str("Buyback quote failed");
        };
        let min_out = self.min_out(quote);
        let msg = swap_msg(self.dex.claw_pool_id, &self.dex.wrap_id, &self.token_id, amount.0, min_out);
        Promise::new(self.dex.wrap_id.clone())
            .function_call(
                "ft_transfer_call".to_string(),
                serde_json::to_vec(&serde_json::json!({ "receiver_id": self.dex.dex_id, "amount": amount, "msg": msg })).unwrap(),
                ONE_YOCTO,
                GAS_FT_TRANSFER_CALL,
            )
            .then(self.view(&self.token_id, "ft_balance_of", serde_json::json!({ "account_id": env::current_account_id() })))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_bought(amount, U128(before)))
    }

    /// Burns what the swap bought. Fee CLAW arriving in the same window could be counted
    /// too; that is bounded by one call's duration and only ever burns, never pays out.
    #[private]
    pub fn on_bought(&mut self, near_in: U128, before: U128) -> Option<Promise> {
        self.buying_back = false;
        let after = read_u128(0).unwrap_or(before.0);
        let bought = after.saturating_sub(before.0);
        emit_event("buyback", &serde_json::json!({ "near_in": near_in, "claw_bought": U128(bought) }));
        if bought == 0 {
            // Swap failed and wNEAR was refunded: unwrap() will move it back to pending.
            return None;
        }
        self.totals.claw_burned.0 += bought;
        Some(Promise::new(self.token_id.clone()).function_call(
            "ft_burn".to_string(),
            serde_json::to_vec(&serde_json::json!({ "amount": U128(bought) })).unwrap(),
            ONE_YOCTO,
            GAS_SIMPLE,
        ))
    }

    // ---------------------------------------------------------------- market cap gate

    /// Price CLAW in USD from Rhea/Ref pools and track consecutive days at or above the
    /// threshold. Three such days unlock the fee-mode vote for good.
    pub fn observe_market_cap(&mut self) -> Promise {
        let token = self.token_id.clone();
        let wrap = self.dex.wrap_id.clone();
        let usd = self.dex.usd_token_id.clone();
        self.quote(self.dex.claw_pool_id, &token, NEAR_ONE, &wrap)
            .and(self.quote(self.dex.usd_pool_id, &wrap, NEAR_ONE, &usd))
            .and(self.view(&token, "ft_total_supply", serde_json::json!({})))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_market_cap())
    }

    #[private]
    pub fn on_market_cap(&mut self) -> U128 {
        let (Some(near_per_claw), Some(usd_per_near), Some(supply)) = (read_u128(0), read_u128(1), read_u128(2)) else {
            env::panic_str("Price read failed");
        };
        // supply (yocto CLAW) * wNEAR per CLAW / 1e24 = market cap in yoctoNEAR; then to USD units.
        let mcap_near = mul_div(supply, near_per_claw, NEAR_ONE);
        let mcap_usd_units = mul_div(mcap_near, usd_per_near, NEAR_ONE);
        let mcap_usd = mcap_usd_units / 10u128.pow(self.dex.usd_decimals as u32);
        self.last_mcap_usd = mcap_usd;
        self.record_observation(env::block_timestamp() / DAY_NS, mcap_usd);
        U128(mcap_usd)
    }

    // ---------------------------------------------------------------- governance

    /// Tier 2 `SetFeeMode`. Only after the market-cap gate has opened.
    pub fn set_mode(&mut self, mode: FeeMode) {
        assert_caller(&self.governor_id, "the Governor");
        require!(self.mode_unlocked, "Fee mode vote unlocks at the market cap threshold");
        self.mode = mode;
        emit_event("fee_mode_set", &serde_json::json!({ "mode": mode }));
    }

    /// Tier 1. The dev share can only stay at or below 20%.
    pub fn set_dev_wallet(&mut self, dev_wallet: AccountId, dev_bps: u16) {
        assert_caller(&self.governor_id, "the Governor");
        require!(dev_bps <= MAX_DEV_BPS, "Dev share cannot exceed 20%");
        self.dev_wallet = dev_wallet.clone();
        self.dev_bps = dev_bps;
        emit_event("dev_wallet_set", &serde_json::json!({ "dev_wallet": dev_wallet, "dev_bps": dev_bps }));
    }

    /// Tier 1.
    pub fn set_limits(&mut self, limits: Limits) {
        assert_caller(&self.governor_id, "the Governor");
        require!(limits.max_slippage_bps <= 500, "Slippage above 5%");
        self.limits = limits.clone();
        emit_event("fee_limits_set", &limits);
    }

    pub fn get_state(&self) -> FeeState {
        FeeState {
            mode: self.mode,
            mode_unlocked: self.mode_unlocked,
            mcap_threshold_usd: U128(self.mcap_threshold_usd),
            streak_days: self.streak_days,
            last_mcap_usd: U128(self.last_mcap_usd),
            dev_wallet: self.dev_wallet.clone(),
            dev_bps: self.dev_bps,
            pending_near: U128(self.pending_near),
            buyback_near: U128(self.buyback_near),
            totals: self.totals.clone(),
            dex: self.dex.clone(),
            limits: self.limits.clone(),
        }
    }
}

impl Contract {
    fn view(&self, contract: &AccountId, method: &str, args: serde_json::Value) -> Promise {
        Promise::new(contract.clone()).function_call(method.to_string(), serde_json::to_vec(&args).unwrap(), NearToken::from_yoctonear(0), GAS_VIEW)
    }

    fn quote(&self, pool_id: u64, token_in: &AccountId, amount_in: u128, token_out: &AccountId) -> Promise {
        self.view(
            &self.dex.dex_id,
            "get_return",
            serde_json::json!({ "pool_id": pool_id, "token_in": token_in, "amount_in": U128(amount_in), "token_out": token_out }),
        )
    }

    fn min_out(&self, quote: u128) -> u128 {
        mul_div(quote, 10_000 - self.limits.max_slippage_bps as u128, 10_000)
    }

    fn record_observation(&mut self, day: u64, mcap_usd: u128) {
        let qualifies = mcap_usd >= self.mcap_threshold_usd;
        if !qualifies {
            self.streak_days = 0;
        } else if day == self.last_observed_day && self.streak_days > 0 {
            // Already counted today.
        } else if day == self.last_observed_day + 1 {
            self.streak_days = self.streak_days.saturating_add(1);
        } else {
            self.streak_days = 1;
        }
        if qualifies {
            self.last_observed_day = day;
        }
        if !self.mode_unlocked && self.streak_days >= UNLOCK_STREAK_DAYS {
            self.mode_unlocked = true;
            emit_event("fee_mode_unlocked", &serde_json::json!({ "mcap_usd": U128(mcap_usd) }));
        }
        emit_event("market_cap_observed", &serde_json::json!({
            "mcap_usd": U128(mcap_usd), "day": day, "streak_days": self.streak_days, "unlocked": self.mode_unlocked,
        }));
    }
}

/// Rhea/Ref instant swap message for `ft_transfer_call`: output is sent back to the sender.
pub fn swap_msg(pool_id: u64, token_in: &AccountId, token_out: &AccountId, amount_in: u128, min_out: u128) -> String {
    serde_json::json!({
        "force": 0,
        "actions": [{
            "pool_id": pool_id,
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": U128(amount_in),
            "min_amount_out": U128(min_out),
        }],
    })
    .to_string()
}

fn read_u128(idx: u64) -> Option<u128> {
    env::promise_result_checked(idx, 128).ok().and_then(|b| serde_json::from_slice::<U128>(&b).ok()).map(|v| v.0)
}

fn succeeded(idx: u64) -> bool {
    !matches!(env::promise_result_checked(idx, 1_024), Err(PromiseError::Failed))
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, PromiseResult};

    fn ok(v: u128) -> PromiseResult {
        PromiseResult::Successful(serde_json::to_vec(&U128(v)).unwrap())
    }

    fn setup(ctx: &mut VMContextBuilder) -> Contract {
        testing_env!(ctx
            .current_account_id("fees.claw.near".parse().unwrap())
            .account_balance(NearToken::from_near(1_000))
            .predecessor_account_id(accounts(0))
            .build());
        Contract::new(
            "claw.near".parse().unwrap(),
            accounts(0),
            "agent.claw.near".parse().unwrap(),
            "ve.claw.near".parse().unwrap(),
            DexConfig {
                dex_id: "v2.ref-finance.near".parse().unwrap(),
                wrap_id: "wrap.near".parse().unwrap(),
                claw_pool_id: 9000,
                usd_pool_id: 3879,
                usd_token_id: "usdt.tether-token.near".parse().unwrap(),
                usd_decimals: 6,
            },
            Limits { max_swap_per_call: U128(1_000 * NEAR_ONE), max_buyback_per_call: U128(100 * NEAR_ONE), max_slippage_bps: 100 },
            accounts(4),
            U128(100_000),
        )
    }

    fn with_results(ctx: &mut VMContextBuilder, results: Vec<PromiseResult>) {
        testing_env!(
            ctx.predecessor_account_id("fees.claw.near".parse().unwrap()).build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            results
        );
    }

    #[test]
    fn distribute_splits_80_20_to_hos_by_default() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.pending_near = 10 * NEAR_ONE;
        c.distribute();
        let s = c.get_state();
        assert_eq!(s.totals.to_dev.0, 2 * NEAR_ONE);
        assert_eq!(s.pending_near.0, 0);
        assert_eq!(s.mode, FeeMode::StakeHoS);
        with_results(&mut ctx, vec![PromiseResult::Successful(vec![]), PromiseResult::Successful(vec![])]);
        c.on_routed(U128(8 * NEAR_ONE), FeeMode::StakeHoS);
        assert_eq!(c.get_state().totals.to_hos.0, 8 * NEAR_ONE);
    }

    #[test]
    fn failed_route_returns_to_pending() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        with_results(&mut ctx, vec![PromiseResult::Successful(vec![]), PromiseResult::Failed]);
        c.on_routed(U128(8 * NEAR_ONE), FeeMode::StakeHoS);
        assert_eq!(c.get_state().pending_near.0, 8 * NEAR_ONE);
    }

    #[test]
    #[should_panic(expected = "unlocks at the market cap threshold")]
    fn mode_locked_before_threshold() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.set_mode(FeeMode::Buyback);
    }

    #[test]
    fn three_consecutive_days_unlock_the_vote() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.record_observation(10, 150_000);
        c.record_observation(10, 150_000); // same day counts once
        c.record_observation(11, 120_000);
        assert!(!c.mode_unlocked);
        c.record_observation(12, 101_000);
        assert!(c.mode_unlocked);
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.set_mode(FeeMode::HolderRewards);
        assert_eq!(c.get_state().mode, FeeMode::HolderRewards);
    }

    #[test]
    fn dip_resets_the_streak() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.record_observation(1, 200_000);
        c.record_observation(2, 200_000);
        c.record_observation(3, 90_000);
        c.record_observation(4, 200_000);
        c.record_observation(6, 200_000); // gap restarts
        assert_eq!(c.streak_days, 1);
        assert!(!c.mode_unlocked);
    }

    #[test]
    fn market_cap_math() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        // 1e9 CLAW supply, 0.00005 NEAR per CLAW, $2.50 per NEAR => $125,000.
        with_results(&mut ctx, vec![ok(NEAR_ONE / 20_000), ok(2_500_000), ok(1_000_000_000 * NEAR_ONE)]);
        assert_eq!(c.on_market_cap().0, 125_000);
        assert_eq!(c.streak_days, 1);
    }

    #[test]
    #[should_panic(expected = "cannot exceed 20%")]
    fn dev_share_capped() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.set_dev_wallet(accounts(3), 2_001);
    }

    #[test]
    fn buyback_mode_queues_near() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.mode_unlocked = true;
        c.set_mode(FeeMode::Buyback);
        c.pending_near = 5 * NEAR_ONE;
        c.distribute();
        assert_eq!(c.get_state().buyback_near.0, 4 * NEAR_ONE);
    }

    #[test]
    fn launcher_sets_pools_once() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.launch_set_pools(42, 3879);
        assert_eq!(c.get_state().dex.claw_pool_id, 42);
    }

    #[test]
    #[should_panic(expected = "Only the launcher, once")]
    fn launcher_cannot_repeat() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.launch_set_pools(42, 3879);
        c.launch_set_pools(43, 3879);
    }

    #[test]
    fn swap_msg_shape() {
        let m: serde_json::Value = serde_json::from_str(&swap_msg(7, &"a.near".parse().unwrap(), &"b.near".parse().unwrap(), 10, 9)).unwrap();
        assert_eq!(m["actions"][0]["pool_id"], 7);
        assert_eq!(m["actions"][0]["min_amount_out"], "9");
    }
}
