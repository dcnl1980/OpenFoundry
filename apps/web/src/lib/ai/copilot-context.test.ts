import { describe, expect, it } from 'vitest';

import { settleList } from './copilot-context';

describe('settleList', () => {
	it('returns the list payload when the request succeeds', async () => {
		const data = await settleList(Promise.resolve({ data: [{ id: 'ds-1' }] }));

		expect(data).toEqual([{ id: 'ds-1' }]);
	});

	it('returns an empty list when a context request fails', async () => {
		const data = await settleList(Promise.reject(new Error('dataset-service down')));

		expect(data).toEqual([]);
	});
});
