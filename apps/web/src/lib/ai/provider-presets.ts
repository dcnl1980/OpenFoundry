export type ProviderPresetId = 'openai' | 'openrouter' | 'anthropic' | 'ollama';

export interface ProviderPreset {
	provider_type: ProviderPresetId;
	model_name: string;
	endpoint_url: string;
	api_mode: string;
	credential_reference: string;
}

const PRESETS: Record<ProviderPresetId, ProviderPreset> = {
	openai: {
		provider_type: 'openai',
		model_name: 'gpt-4.1-mini',
		endpoint_url: 'https://api.openai.com/v1',
		api_mode: 'chat_completions',
		credential_reference: 'OPENAI_API_KEY',
	},
	openrouter: {
		provider_type: 'openrouter',
		model_name: 'openai/gpt-4o-mini',
		endpoint_url: 'https://openrouter.ai/api/v1',
		api_mode: 'chat_completions',
		credential_reference: 'OPENROUTER_API_KEY',
	},
	anthropic: {
		provider_type: 'anthropic',
		model_name: 'claude-3.7-sonnet',
		endpoint_url: 'https://api.anthropic.com/v1',
		api_mode: 'messages',
		credential_reference: 'ANTHROPIC_API_KEY',
	},
	ollama: {
		provider_type: 'ollama',
		model_name: 'llama3.1:8b',
		endpoint_url: 'http://localhost:11434/api',
		api_mode: 'chat',
		credential_reference: '',
	},
};

export const PROVIDER_PRESET_IDS = Object.keys(PRESETS) as ProviderPresetId[];

export function providerPreset(id: string): ProviderPreset {
	return PRESETS[(id as ProviderPresetId)] ?? PRESETS.openai;
}

export function applyProviderPreset<T extends ProviderPreset>(current: T, id: string): T {
	return {
		...current,
		...providerPreset(id),
	};
}
