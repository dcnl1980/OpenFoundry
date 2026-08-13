export const PROPERTY_TYPES = [
	'string',
	'integer',
	'float',
	'boolean',
	'date',
	'timestamp',
	'json',
	'array',
	'reference',
] as const;

export type PropertyTypeName = (typeof PROPERTY_TYPES)[number];

export interface PropertyFormInput {
	name: string;
	display_name: string;
	description: string;
	property_type: string;
	required: boolean;
	unique_constraint: boolean;
}

export type CreatePropertyBody = {
	name: string;
	display_name: string;
	description: string;
	property_type: string;
	required: boolean;
	unique_constraint: boolean;
};

export function buildCreatePropertyBody(
	input: PropertyFormInput,
): { ok: true; body: CreatePropertyBody } | { ok: false; error: string } {
	const name = input.name.trim();
	if (!name) {
		return { ok: false, error: 'Property name is required' };
	}

	const propertyType = PROPERTY_TYPES.includes(input.property_type as PropertyTypeName)
		? input.property_type
		: '';
	if (!propertyType) {
		return { ok: false, error: 'Select a valid property type' };
	}

	return {
		ok: true,
		body: {
			name,
			display_name: input.display_name.trim() || name,
			description: input.description.trim(),
			property_type: propertyType,
			required: input.required,
			unique_constraint: input.unique_constraint,
		},
	};
}
