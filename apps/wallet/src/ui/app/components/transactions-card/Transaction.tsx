// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
    getTransferObjectTransaction,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import { getEventsSummary, getAmount } from '_helpers';
import { useMiddleEllipsis } from '_hooks';
import { getTxnEffectsEventID } from '_redux/slices/txresults';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

export function Transaction({
    txn,
    address,
}: {
    txn: SuiTransactionResponse;
    address: SuiAddress;
}) {
    const { certificate } = txn;
    const executionStatus = getExecutionStatusType(txn);
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);
    const { coins: eventsSummary } = getEventsSummary(txn.effects, address);

    const objectId = useMemo(() => {
        const transferId = getTransferObjectTransaction(
            certificate.data.transactions[0]
        )?.objectRef?.objectId;
        return transferId
            ? transferId
            : getTxnEffectsEventID(txn.effects, address)[0];
    }, [address, certificate.data.transactions, txn.effects]);

    const amountByRecipient = getAmount(
        certificate.data.transactions[0],
        txn.effects
    );

    const amount = useMemo(() => {
        const amount = amountByRecipient && amountByRecipient?.[0]?.amount;
        const amountTransfers = eventsSummary.reduce(
            (acc, { amount }) => acc + amount,
            0
        );

        return Math.abs(amount || amountTransfers);
    }, [amountByRecipient, eventsSummary]);

    const recipientAddress = useMemo(() => {
        const transferObjectRecipientAddress =
            amountByRecipient &&
            amountByRecipient?.find(
                ({ recipientAddress }) => recipientAddress !== address
            )?.recipientAddress;
        const receiverAddr =
            eventsSummary &&
            eventsSummary.find(
                ({ receiverAddress }) => receiverAddress !== address
            )?.receiverAddress;

        return (
            receiverAddr ??
            transferObjectRecipientAddress ??
            certificate.data.sender
        );
    }, [address, amountByRecipient, certificate.data.sender, eventsSummary]);

    const isSender = address === certificate.data.sender;

    const receiverAddress = useMiddleEllipsis(
        recipientAddress,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );

    const error = useMemo(() => getExecutionStatusError(txn), [txn]);

    const isSuiTransfer =
        txnKind === 'PaySui' ||
        txnKind === 'TransferSui' ||
        txnKind === 'PayAllSui' ||
        txnKind === 'Pay';

    const isTransfer = isSuiTransfer || txnKind === 'TransferObject';

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_add_delegation_mul_coin'
        )
            return 'Staked';
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_withdraw_delegation'
        )
            return 'UnStaked';
        return 'Call';
    }, [moveCallTxn, txnKind]);

    const txnIcon = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Rewards';
        if (moveCallLabel && moveCallLabel !== 'Call') return moveCallLabel;
        return isSender ? 'Send' : 'Received';
    }, [isSender, moveCallLabel, txnKind]);

    const txnLabel = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Received Staking Rewards';
        if (moveCallLabel) return moveCallLabel;
        return isSender ? 'Sent' : 'Received';
    }, [txnKind, moveCallLabel, isSender]);

    // Show sui symbol only if it is a sui transfer or if or staking or unstaking
    const showSuiSymbol =
        isSuiTransfer || (moveCallLabel && moveCallLabel !== 'Call');

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: txn.certificate.transactionDigest,
            }).toString()}`}
            className="flex items-center w-full flex-col gap-2 py-4 no-underline"
        >
            <div className="flex items-start w-full justify-between gap-3">
                <div className="w-7.5">
                    <TxnIcon
                        txnFailed={executionStatus !== 'success' || !!error}
                        variant={txnIcon}
                    />
                </div>
                <div className="flex flex-col w-full gap-1.5">
                    {error ? (
                        <div className="flex flex-col w-full gap-1.5">
                            <Text color="gray-90" weight="semibold">
                                Transaction Failed
                            </Text>
                            <div className="flex break-all text-issue-dark text-subtitle">
                                {error}
                            </div>
                        </div>
                    ) : (
                        <div className="flex w-full justify-between flex-col ">
                            <div className="flex w-full justify-between">
                                <div className="flex gap-1 align-middle items-baseline">
                                    <Text color="gray-90" weight="semibold">
                                        {txnLabel}
                                    </Text>
                                    {showSuiSymbol && (
                                        <Text
                                            color="gray-90"
                                            weight="normal"
                                            variant="subtitleSmall"
                                        >
                                            SUI
                                        </Text>
                                    )}
                                </div>
                                <CoinBalance amount={amount} />
                            </div>
                            <div className="flex flex-col w-full gap-1.5">
                                <TxnTypeLabel
                                    address={receiverAddress}
                                    moveCallFnName={moveCallTxn?.function}
                                    isSender={isSender}
                                    isTransfer={isTransfer}
                                />
                                {objectId && <TxnImage id={objectId} />}
                            </div>
                        </div>
                    )}

                    {txn.timestamp_ms && (
                        <DateCard timestamp={txn.timestamp_ms} size="sm" />
                    )}
                </div>
            </div>
        </Link>
    );
}
