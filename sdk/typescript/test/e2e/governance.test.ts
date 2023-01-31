// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, expectTypeOf } from 'vitest';
import {
  LocalTxnDataSerializer,
  RawSigner,
  DelegatedStake,
  ObjectId,
  normalizeSuiObjectId,
} from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Governance API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  // let packageId: string;
  // let shouldSkip: boolean;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
  });

  it('test requestAddDelegation', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    let delcoins: ObjectId[] = [];

    delcoins.push(normalizeSuiObjectId(coins[0].objectId));
    delcoins.push(normalizeSuiObjectId(coins[2].objectId));

    console.log(delcoins);

    const tx = {
      coins: delcoins,
      amount: '10',
      validator: normalizeSuiObjectId('0x1'),
      gasBudget: 10000,
    };

    console.log(tx);

    const radd = await signer.requestAddDelegation(tx);
  });
  it('test getDelegatedStakes', async () => {
    const stakes = await toolbox.provider.getDelegatedStakes(toolbox.address());
    expectTypeOf(stakes).toBeArray(DelegatedStake);
    //not able to test this, needs address with stake
  });
  it('test getValidators', async () => {
    const validators = await toolbox.provider.getValidators();
    expect(validators.length).greaterThan(0);
  });
  it('test getCommiteeInfo', async () => {
    const commiteeInfo = await toolbox.provider.getCommitteeInfo(0);
    expect(commiteeInfo.committee_info?.length).greaterThan(0);

    const commiteeInfo2 = await toolbox.provider.getCommitteeInfo(100);
    expect(commiteeInfo2.committee_info).to.toEqual(null);
  });
  it('test getSuiSystemState', async () => {
    const systemState = await toolbox.provider.getSuiSystemState();
    expect(systemState.epoch).toBeGreaterThanOrEqual(0);
  });
});
