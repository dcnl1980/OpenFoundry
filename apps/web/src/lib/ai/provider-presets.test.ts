import { describe, expect, it } from 'vitest';

import { applyProviderPreset, providerPreset } from './provider-presets';

describe('provider presets', () => {
	it('uses the OpenRouter chat-completions endpoint and env credential', () => {
		expect(providerPreset('openrouter')).toMatchObject({
			provider_type: 'openrouter',
			model_name: 'openai/gpt-4o-mini',
			endpoint_url: 'https://openrouter.ai/api/v1',
			api_mode: 'chat_completions',
			credential_reference: 'OPENROUTER_API_KEY',
		});
	});

	it('keeps the current name when applying a preset', () => {
		const next = applyProviderPreset(
			{
				name: 'Ops Router',
				provider_type: 'openai',
				model_name: 'gpt-4.1-mini',
				endpoint_url: 'https://api.openai.com/v1',
				api_mode: 'chat_completions',
				credential_reference: 'OPENAI_API_KEY',
			},
			'openrouter',
		);

		expect(next.name).toBe('Ops Router');
		expect(next.endpoint_url).toBe('https://openrouter.ai/api/v1');
		expect(next.credential_reference).toBe('OPENROUTER_API_KEY');
	});
});
