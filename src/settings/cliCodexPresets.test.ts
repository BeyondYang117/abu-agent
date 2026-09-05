import { describe, expect, it } from 'vitest'
import {
  applyCodexPreset,
  buildCodexProviderConfigToml,
  CODEX_CUSTOM_PRESET_ID,
  CODEX_PRESET_BUTTONS,
  CODEX_RELAY_PRESETS,
  detectCodexPresetId,
  extractCodexBaseUrl,
  extractCodexModel,
  extractOpenAiApiKey,
  initialCodexTomlAuth,
  readCodexCompactionFields,
  setCodexCompactionFields,
  setCodexStructuredFields,
  validateCodexConfigToml,
  DEFAULT_CODEX_AUTH_JSON,
  DEFAULT_CODEX_CONFIG_TOML,
} from './cliCodexPresets'

describe('cliCodexPresets', () => {
  it('buildCodexProviderConfigToml always includes provider name', () => {
    const toml = buildCodexProviderConfigToml('relay', 'https://example.com/v1', 'gpt-x', 'chat', 'relay')
    expect(toml).toContain('name = "relay"')
    expect(toml).toContain('base_url = "https://example.com/v1"')
    expect(toml).toContain('wire_api = "chat"')
    expect(toml).toContain('model_provider = "relay"')
  })

  it('every relay preset has matching base_url extractable for detect', () => {
    for (const preset of CODEX_RELAY_PRESETS) {
      const url = extractCodexBaseUrl(preset.configToml)
      expect(url.length, preset.id).toBeGreaterThan(0)
      expect(detectCodexPresetId(preset.configToml)).toBe(preset.id)
    }
  })

  it('detectCodexPresetId falls back to custom', () => {
    expect(detectCodexPresetId('')).toBe(CODEX_CUSTOM_PRESET_ID)
    expect(detectCodexPresetId('model = "x"\n')).toBe(CODEX_CUSTOM_PRESET_ID)
    expect(detectCodexPresetId('base_url = "https://unknown.example/v1"')).toBe(CODEX_CUSTOM_PRESET_ID)
  })

  it('applyCodexPreset preserves OPENAI_API_KEY across switches', () => {
    const withKey = '{\n  "OPENAI_API_KEY": "sk-keep"\n}'
    const next = applyCodexPreset('deepseek', withKey)
    expect(next.configToml).toContain('deepseek.com')
    expect(JSON.parse(next.authJson).OPENAI_API_KEY).toBe('sk-keep')
  })

  it('preset buttons start with custom', () => {
    expect(CODEX_PRESET_BUTTONS[0]?.id).toBe(CODEX_CUSTOM_PRESET_ID)
  })

  it('default custom template model is gpt-5.5', () => {
    const empty = initialCodexTomlAuth(null)
    expect(empty.configToml).toContain('model = "gpt-5.5"')
    expect(extractCodexModel(empty.configToml)).toBe('gpt-5.5')
  })

  it('initialCodexTomlAuth uses defaults for empty, keeps existing', () => {
    const empty = initialCodexTomlAuth(null)
    expect(empty.configToml).toContain('model_providers')
    expect(empty.authJson).toContain('OPENAI_API_KEY')

    const kept = initialCodexTomlAuth({ configToml: 'model = "a"', authJson: '{"k":1}' })
    expect(kept.configToml).toBe('model = "a"')
    expect(kept.authJson).toBe('{"k":1}')
  })

  it('setCodexStructuredFields patches url / model / key and keeps wire_api', () => {
    const deepseek = CODEX_RELAY_PRESETS.find((p) => p.id === 'deepseek')!
    const next = setCodexStructuredFields(deepseek.configToml, deepseek.authJson, {
      baseUrl: 'https://my.proxy/v1',
      model: 'gpt-5.5',
      apiKey: 'sk-test',
    })
    expect(extractCodexBaseUrl(next.configToml)).toBe('https://my.proxy/v1')
    expect(extractCodexModel(next.configToml)).toBe('gpt-5.5')
    expect(next.configToml).toContain('wire_api = "chat"')
    expect(extractOpenAiApiKey(next.authJson)).toBe('sk-test')
  })

  it('preserves compaction overrides when structured fields or presets change', () => {
    const configured = setCodexCompactionFields(DEFAULT_CODEX_CONFIG_TOML, {
      contextWindow: '272000',
      autoCompactLimit: '217000',
      autoCompactScope: 'body_after_prefix',
    })
    const patched = setCodexStructuredFields(configured, DEFAULT_CODEX_AUTH_JSON, {
      model: 'gpt-5.4',
    })
    expect(readCodexCompactionFields(patched.configToml)).toEqual({
      contextWindow: '272000',
      autoCompactLimit: '217000',
      autoCompactScope: 'body_after_prefix',
    })

    const switched = applyCodexPreset('deepseek', DEFAULT_CODEX_AUTH_JSON, patched.configToml)
    expect(readCodexCompactionFields(switched.configToml)).toEqual({
      contextWindow: '272000',
      autoCompactLimit: '217000',
      autoCompactScope: 'body_after_prefix',
    })
  })

  it('omits compaction keys by default and removes scope when the limit is cleared', () => {
    expect(DEFAULT_CODEX_CONFIG_TOML).not.toContain('model_context_window')
    expect(DEFAULT_CODEX_CONFIG_TOML).not.toContain('model_auto_compact_token_limit')

    const configured = setCodexCompactionFields(DEFAULT_CODEX_CONFIG_TOML, {
      autoCompactLimit: '210000',
      autoCompactScope: 'body_after_prefix',
    })
    const cleared = setCodexCompactionFields(configured, { autoCompactLimit: '' })
    expect(cleared).not.toContain('model_auto_compact_token_limit')
    expect(cleared).not.toContain('model_auto_compact_token_limit_scope')
  })

  it('does not write the default total scope for older Codex versions', () => {
    const configured = setCodexCompactionFields(DEFAULT_CODEX_CONFIG_TOML, {
      contextWindow: '272000',
      autoCompactLimit: '217000',
      autoCompactScope: 'total',
    })
    expect(configured).toContain('model_auto_compact_token_limit = 217000')
    expect(configured).not.toContain('model_auto_compact_token_limit_scope')
  })

  it('validates compaction overrides in raw TOML', () => {
    const valid = setCodexCompactionFields(DEFAULT_CODEX_CONFIG_TOML, {
      contextWindow: '272000',
      autoCompactLimit: '217000',
    })
    expect(validateCodexConfigToml(valid)).toBeNull()
    expect(validateCodexConfigToml(valid.replace('217000', '272000'))).toBe('invalid_auto_compact_range')
    expect(validateCodexConfigToml(valid.replace('272000', 'oops'))).toBe('invalid_context_window')
  })

  it('validateCodexConfigToml accepts defaults and rejects missing keys', () => {
    expect(validateCodexConfigToml(DEFAULT_CODEX_CONFIG_TOML)).toBeNull()
    expect(validateCodexConfigToml('')).toBe('empty')
    expect(validateCodexConfigToml('model = "x"')).toBe('missing_model_provider')
    expect(validateCodexConfigToml('model = "x"\nmodel_provider = "r"')).toBe('missing_base_url')
    expect(validateCodexConfigToml('model = "unterminated')).toMatch(/^unclosed_string/)
  })
})
