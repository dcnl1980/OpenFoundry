import { describe, expect, it } from 'vitest';

import { ApiError, emptyOnNotFound } from './client';

describe('emptyOnNotFound', () => {
	it('returns the fallback when the API reports 404', () => {
		const recover = emptyOnNotFound<string[]>([]);

		expect(recover(new ApiError(404, 'unknown service route'))).toEqual([]);
	});

	it('rethrows other API errors', () => {
		const recover = emptyOnNotFound<string[]>([]);

		expect(() => recover(new ApiError(500, 'upstream unavailable'))).toThrow('upstream unavailable');
	});
});
