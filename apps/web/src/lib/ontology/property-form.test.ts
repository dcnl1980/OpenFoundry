import { describe, expect, it } from 'vitest';

import { buildCreatePropertyBody } from './property-form';

describe('buildCreatePropertyBody', () => {
	it('rejects a blank property name', () => {
		const result = buildCreatePropertyBody({
			name: '   ',
			display_name: 'Status',
			description: '',
			property_type: 'string',
			required: true,
			unique_constraint: false,
		});

		expect(result).toEqual({ ok: false, error: 'Property name is required' });
	});

	it('builds the create payload with a defaulted display name', () => {
		const result = buildCreatePropertyBody({
			name: 'status',
			display_name: '',
			description: 'Lifecycle state',
			property_type: 'string',
			required: true,
			unique_constraint: false,
		});

		expect(result).toEqual({
			ok: true,
			body: {
				name: 'status',
				display_name: 'status',
				description: 'Lifecycle state',
				property_type: 'string',
				required: true,
				unique_constraint: false,
			},
		});
	});
});
