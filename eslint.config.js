import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import pluginVue from 'eslint-plugin-vue';
import globals from 'globals';

export default tseslint.config(
  // 1. 全局忽略配置 (必须放在首位)
  {
    ignores: ['src-tauri/**', 'dist/**', 'public/**', '.vscode/**', 'node_modules/**'],
  },

  // 2. 基础推荐配置 (直接作为参数传入即可，新版会自动扁平化)
  eslint.configs.recommended,
  
  // 3. TypeScript 推荐配置 (新版不再需要使用 ... 展开数组)
  tseslint.configs.recommended,

  // 4. Vue 推荐配置 (同理，直接传入数组对象)
  pluginVue.configs['flat/recommended'],

  // 5. 核心严格规则配置 (业务逻辑与复杂度门禁)
  {
    files: ['**/*.{js,ts,vue}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: ['.vue'],
        sourceType: 'module',
      },
    },
    rules: {
      // --- 核心复杂度报错设定 (业内严格模式) ---
      'complexity': ['error', 15],
      'max-depth': ['error', 4],
      'max-lines-per-function': ['error', { 
        max: 50, 
        skipBlankLines: true, 
        skipComments: true 
      }],
      'max-params': ['error', 4],

      // --- 代码质量严格规范 ---
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': 'error',
      'vue/no-mutating-props': 'error',

      // --- 辅助质量规则 ---
      'vue/html-self-closing': 'error',
      'vue/max-attributes-per-line': ['warn', {
        singleline: 3,
        multiline: 1
      }],
    },
  }
);
