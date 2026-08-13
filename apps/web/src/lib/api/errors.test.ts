import { describe, expect, it } from 'vitest';

import { ApiError, emptyOnNotFound, errorMessageFromBody } from './client';

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

describe('errorMessageFromBody', () => {
	it('reads the error field from a JSON object', () => {
		expect(errorMessageFromBody({ error: 'column branch_id does not exist' }, 'Unknown error')).toBe(
			'column branch_id does not exist',
		);
	});

	it('uses a plain string body from older handlers', () => {
		expect(errorMessageFromBody('duplicate key value violates unique constraint', 'Unknown error')).toBe(
			'duplicate key value violates unique constraint',
		);
	});
});
