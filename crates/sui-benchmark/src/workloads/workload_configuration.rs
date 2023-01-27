// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::options::{Opts, RunSpec};
use crate::system_state_observer::SystemStateObserver;
use crate::util::generate_all_gas_for_test;
use crate::workloads::delegation::DelegationWorkload;
use crate::workloads::shared_counter::SharedCounterWorkload;
use crate::workloads::transfer_object::TransferObjectWorkload;
use crate::workloads::workload::WorkloadInfo;
use crate::workloads::{
    make_combination_workload, make_delegation_workload, make_shared_counter_workload,
    make_transfer_object_workload, split_workload, Gas, WorkloadGasConfig, WorkloadInitGas,
    WorkloadPayloadGas,
};
use crate::ValidatorProxy;
use anyhow::Result;
use move_core_types::language_storage::TypeTag;
use std::sync::Arc;

pub enum WorkloadConfiguration {
    // Each worker runs all workloads with similar configuration. Backpressure for one workload impact others
    Combined,
    // Different workers run with different workloads configuration, backpressure on one does not impact others
    Disjoint,
}

impl WorkloadConfiguration {
    pub async fn configure(
        &self,
        gas: Gas,
        pay_coin: Gas,
        pay_coin_type_tag: TypeTag,
        proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
        opts: &Opts,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)>> {
        match opts.run_spec {
            RunSpec::Bench {
                target_qps,
                num_workers,
                in_flight_ratio,
                shared_counter,
                transfer_object,
                delegation,
                shared_counter_hotness_factor,
                ..
            } => match self {
                WorkloadConfiguration::Combined => {
                    self.configure_combined_mode(
                        num_workers,
                        opts.num_transfer_accounts,
                        shared_counter,
                        transfer_object,
                        delegation,
                        shared_counter_hotness_factor,
                        target_qps,
                        in_flight_ratio,
                        gas,
                        pay_coin,
                        pay_coin_type_tag,
                        proxies,
                        system_state_observer,
                    )
                    .await
                }
                WorkloadConfiguration::Disjoint => {
                    self.configure_disjoint_mode(
                        num_workers,
                        opts.num_transfer_accounts,
                        shared_counter,
                        transfer_object,
                        delegation,
                        shared_counter_hotness_factor,
                        target_qps,
                        in_flight_ratio,
                        gas,
                        pay_coin,
                        pay_coin_type_tag,
                        proxies,
                        system_state_observer,
                    )
                    .await
                }
            },
        }
    }

    async fn configure_combined_mode(
        &self,
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        shared_counter_hotness_factor: u32,
        target_qps: u64,
        in_flight_ratio: u64,
        gas: Gas,
        coin: Gas,
        coin_type_tag: TypeTag,
        proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)>> {
        Ok(configure_combined_mode_helper(
            shared_counter_hotness_factor,
            target_qps,
            in_flight_ratio,
            shared_counter_weight,
            transfer_object_weight,
            num_transfer_accounts,
            delegation_weight,
            proxies,
            gas,
            coin,
            coin_type_tag,
            num_workers,
            system_state_observer,
        )
        .await?)
    }

    async fn configure_disjoint_mode(
        &self,
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        shared_counter_hotness_factor: u32,
        target_qps: u64,
        in_flight_ratio: u64,
        gas: Gas,
        coin: Gas,
        coin_type_tag: TypeTag,
        proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)>> {
        let shared_counter_weight_ratio = shared_counter_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let shared_counter_qps = (shared_counter_weight_ratio * target_qps as f32) as u64;
        let shared_counter_num_workers =
            (shared_counter_weight_ratio * num_workers as f32).ceil() as u64;
        let shared_counter_max_ops = (shared_counter_qps * in_flight_ratio) as u64;
        let shared_counter_ratio =
            1.0 - (std::cmp::min(shared_counter_hotness_factor as u32, 100) as f32 / 100.0);
        let num_shared_counters = (shared_counter_max_ops as f32 * shared_counter_ratio) as u64;
        let (shared_counter_workload_init_gas_config, shared_counter_workload_payload_gas_config) =
            if shared_counter_qps == 0
                || shared_counter_max_ops == 0
                || shared_counter_num_workers == 0
            {
                (vec![], vec![])
            } else {
                let shared_counter_init_coin_configs =
                    SharedCounterWorkload::generate_coin_config_for_init(num_shared_counters);
                let shared_counter_payload_coin_configs =
                    SharedCounterWorkload::generate_coin_config_for_payloads(
                        shared_counter_max_ops,
                    );
                (
                    shared_counter_init_coin_configs,
                    shared_counter_payload_coin_configs,
                )
            };

        let transfer_object_weight_ratio = transfer_object_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let transfer_object_qps = (transfer_object_weight_ratio * target_qps as f32) as u64;
        let transfer_object_num_workers =
            (transfer_object_weight_ratio * num_workers as f32).ceil() as u64;
        let transfer_object_max_ops = (transfer_object_qps * in_flight_ratio) as u64;

        let delegate_weight_ratio = delegation_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let delegate_qps = (delegate_weight_ratio * target_qps as f32) as u64;
        let delegate_num_workers = (delegate_weight_ratio * num_workers as f32).ceil() as u64;
        let delegate_max_ops = (delegate_qps * in_flight_ratio) as u64;

        let (transfer_object_workload_tokens, transfer_object_workload_payload_gas_config) =
            if transfer_object_qps == 0
                || transfer_object_max_ops == 0
                || transfer_object_num_workers == 0
            {
                (vec![], vec![])
            } else {
                TransferObjectWorkload::generate_coin_config_for_payloads(
                    transfer_object_max_ops,
                    num_transfer_accounts,
                    transfer_object_max_ops,
                )
            };
        let delegation_gas_configs = if delegation_weight > 0 {
            DelegationWorkload::generate_gas_config_for_payloads(num_transfer_accounts)
        } else {
            vec![]
        };

        let mut proxy_workloads: Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)> =
            Vec::new();
        let num_proxies = proxies.len();

        let shared_counter_workload_init_gas_config_chunks =
            split_workload(&shared_counter_workload_init_gas_config, num_proxies);
        let shared_counter_workload_payload_gas_config_chunks =
            split_workload(&shared_counter_workload_payload_gas_config, num_proxies);
        let transfer_object_workload_tokens_chunks =
            split_workload(&transfer_object_workload_tokens, num_proxies);
        let transfer_object_workload_payload_gas_config_chunks =
            split_workload(&transfer_object_workload_payload_gas_config, num_proxies);
        let delegation_gas_configs_chunks = split_workload(&delegation_gas_configs, num_proxies);

        for (i, proxy) in proxies.iter().enumerate() {
            let mut workloads = vec![];
            let workload_gas_config = WorkloadGasConfig {
                shared_counter_workload_init_gas_config:
                    shared_counter_workload_init_gas_config_chunks[i].clone(),
                shared_counter_workload_payload_gas_config:
                    shared_counter_workload_payload_gas_config_chunks[i].clone(),
                transfer_object_workload_tokens: transfer_object_workload_tokens_chunks[i].clone(),
                transfer_object_workload_payload_gas_config:
                    transfer_object_workload_payload_gas_config_chunks[i].clone(),
                delegation_gas_configs: delegation_gas_configs_chunks[i].clone(),
            };

            // Should not have any issues sharing the same primary gas object for generation
            // as these generation is done sequentially for each proxy.
            // TODO(scale): verify stress performance does not degrade because of this.
            let (workload_init_gas, workload_payload_gas) = generate_all_gas_for_test(
                proxies[i].clone(),
                gas.clone(),
                coin.clone(),
                coin_type_tag.clone(),
                workload_gas_config,
                *system_state_observer.reference_gas_price.borrow(),
            )
            .await?;
            if let Some(mut shared_counter_workload) = make_shared_counter_workload(
                shared_counter_qps,
                shared_counter_num_workers,
                shared_counter_max_ops,
                WorkloadPayloadGas {
                    transfer_tokens: vec![],
                    transfer_object_payload_gas: vec![],
                    shared_counter_payload_gas: workload_payload_gas.shared_counter_payload_gas,
                    delegation_payload_gas: vec![],
                },
            ) {
                shared_counter_workload
                    .workload
                    .init(
                        workload_init_gas,
                        proxies[i].clone(),
                        system_state_observer.clone(),
                    )
                    .await;
                workloads.push(shared_counter_workload);
            }
            if let Some(mut transfer_object_workload) = make_transfer_object_workload(
                transfer_object_qps,
                transfer_object_num_workers,
                transfer_object_max_ops,
                num_transfer_accounts,
                WorkloadPayloadGas {
                    transfer_tokens: workload_payload_gas.transfer_tokens,
                    transfer_object_payload_gas: workload_payload_gas.transfer_object_payload_gas,
                    shared_counter_payload_gas: vec![],
                    delegation_payload_gas: vec![],
                },
            ) {
                transfer_object_workload
                    .workload
                    .init(
                        WorkloadInitGas {
                            shared_counter_init_gas: vec![],
                        },
                        proxies[i].clone(),
                        system_state_observer.clone(),
                    )
                    .await;
                workloads.push(transfer_object_workload);
            }
            if let Some(delegation_workload) = make_delegation_workload(
                delegate_qps,
                delegate_num_workers,
                delegate_max_ops,
                WorkloadPayloadGas {
                    transfer_tokens: vec![],
                    transfer_object_payload_gas: vec![],
                    shared_counter_payload_gas: vec![],
                    delegation_payload_gas: workload_payload_gas.delegation_payload_gas,
                },
            ) {
                workloads.push(delegation_workload);
            }

            proxy_workloads.push((proxy.clone(), workloads));
        }
        Ok(proxy_workloads)
    }
}

pub async fn configure_combined_mode_helper(
    shared_counter_hotness_factor: u32,
    target_qps: u64,
    in_flight_ratio: u64,
    shared_counter_weight: u32,
    transfer_object_weight: u32,
    num_transfer_accounts: u64,
    delegation_weight: u32,
    proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
    gas: Gas,
    coin: Gas,
    coin_type_tag: TypeTag,
    num_workers: u64,
    system_state_observer: Arc<SystemStateObserver>,
) -> std::result::Result<
    Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)>,
    anyhow::Error,
> {
    let shared_counter_ratio =
        1.0 - (std::cmp::min(shared_counter_hotness_factor as u32, 100) as f32 / 100.0);
    let max_ops = target_qps * in_flight_ratio;
    let all_shared_counter_coin_configs = if shared_counter_weight == 0 {
        None
    } else {
        let num_shared_counters = (max_ops as f32 * shared_counter_ratio) as u64;
        let shared_counter_init_coin_configs =
            SharedCounterWorkload::generate_coin_config_for_init(num_shared_counters);
        let shared_counter_payload_coin_configs =
            SharedCounterWorkload::generate_coin_config_for_payloads(max_ops);
        Some((
            shared_counter_init_coin_configs,
            shared_counter_payload_coin_configs,
        ))
    };
    let all_transfer_object_coin_configs = if transfer_object_weight == 0 {
        None
    } else {
        Some(TransferObjectWorkload::generate_coin_config_for_payloads(
            max_ops,
            num_transfer_accounts,
            max_ops,
        ))
    };
    let delegation_gas_configs = if delegation_weight > 0 {
        DelegationWorkload::generate_gas_config_for_payloads(max_ops)
    } else {
        vec![]
    };
    let (shared_counter_workload_init_gas_config, shared_counter_workload_payload_gas_config) =
        all_shared_counter_coin_configs.unwrap_or((vec![], vec![]));
    let (transfer_object_workload_tokens, transfer_object_workload_payload_gas_config) =
        all_transfer_object_coin_configs.unwrap_or((vec![], vec![]));

    let mut proxy_workloads: Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)> =
        Vec::new();
    let num_proxies = proxies.len();

    let shared_counter_workload_init_gas_config_chunks =
        split_workload(&shared_counter_workload_init_gas_config, num_proxies);
    let shared_counter_workload_payload_gas_config_chunks =
        split_workload(&shared_counter_workload_payload_gas_config, num_proxies);
    let transfer_object_workload_tokens_chunks =
        split_workload(&transfer_object_workload_tokens, num_proxies);
    let transfer_object_workload_payload_gas_config_chunks =
        split_workload(&transfer_object_workload_payload_gas_config, num_proxies);
    let delegation_gas_configs_chunks = split_workload(&delegation_gas_configs, num_proxies);
    for (i, proxy) in proxies.iter().enumerate() {
        let workload_gas_config = WorkloadGasConfig {
            shared_counter_workload_init_gas_config: shared_counter_workload_init_gas_config_chunks
                [i]
                .clone(),
            shared_counter_workload_payload_gas_config:
                shared_counter_workload_payload_gas_config_chunks[i].clone(),
            transfer_object_workload_tokens: transfer_object_workload_tokens_chunks[i].clone(),
            transfer_object_workload_payload_gas_config:
                transfer_object_workload_payload_gas_config_chunks[i].clone(),
            delegation_gas_configs: delegation_gas_configs_chunks[i].clone(),
        };

        // Should not have any issues sharing the same primary gas object for generation
        // as these generation is done sequentially for each proxy.
        // TODO(scale): verify stress performance does not degrade because of this.
        let (workload_init_gas, workload_payload_gas) = generate_all_gas_for_test(
            proxies[i].clone(),
            gas.clone(),
            coin.clone(),
            coin_type_tag.clone(),
            workload_gas_config,
            *system_state_observer.reference_gas_price.borrow(),
        )
        .await?;

        let mut combination_workload = make_combination_workload(
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            shared_counter_weight,
            transfer_object_weight,
            delegation_weight,
            workload_payload_gas,
        );
        combination_workload
            .workload
            .init(
                workload_init_gas,
                proxies[i].clone(),
                system_state_observer.clone(),
            )
            .await;

        proxy_workloads.push((proxy.clone(), vec![combination_workload]));
    }

    Ok(proxy_workloads)
}
