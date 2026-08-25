import { describe, expect, it } from 'vitest';
import {
  createQuarantineRecord,
  finalizeQuarantine,
  getQuarantineDecision,
  replayQuarantine,
  QUARANTINE_STATES,
} from './quarantine';

function createFakeDb() {
  const records = [];
  const collection = {
    createIndex: async () => {},
    insertOne: async (record) => {
      records.push({ ...record, _id: records.length + 1 });
    },
    findOne: async (query) => records.find((record) => record.contentHash === query.contentHash) || null,
    findOneAndUpdate: async (query, update) => {
      const record = records.find((item) => item.contentHash === query.contentHash && item.state === query.state);
      if (!record) return { value: null };
      Object.assign(record, update.$set);
      record.attemptCount += update.$inc?.attemptCount || 0;
      return { value: record };
    },
    updateOne: async (query, update) => {
      const record = records.find((item) => item._id === query._id || item.contentHash === query.contentHash);
      if (record) Object.assign(record, update.$set);
      return { modifiedCount: record ? 1 : 0 };
    },
    find: () => ({
      limit: () => records.filter((record) => [QUARANTINE_STATES.TIMEOUT, QUARANTINE_STATES.SCANNER_UNAVAILABLE].includes(record.state))[Symbol.iterator](),
    }),
  };

  return { collection: () => collection, records };
}

describe('replayQuarantine', () => {
  it('re-runs scanning so a stuck record reaches a terminal state', async () => {
    const db = createFakeDb();
    await createQuarantineRecord({
      db,
      contentHash: 'QmRecovered',
      fileName: 'notes.pdf',
      mimeType: 'application/pdf',
      sizeBytes: 1024,
      uploaderAddress: 'GCREATOR',
    });
    await finalizeQuarantine({
      db,
      contentHash: 'QmRecovered',
      state: QUARANTINE_STATES.TIMEOUT,
    });

    const results = await replayQuarantine(db, 100, {
      scannerImpl: {
        name: 'test-scanner',
        scan: async () => ({ infected: false }),
      },
      timeoutMs: 1000,
    });

    expect(results).toEqual(['QmRecovered']);
    expect((await getQuarantineDecision(db, 'QmRecovered')).state).toBe(QUARANTINE_STATES.CLEAN);
  });
});
