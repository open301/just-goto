import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import stylistic from '@stylistic/eslint-plugin';
import reactHooks from 'eslint-plugin-react-hooks';
import tseslint from 'typescript-eslint';

// Adapted from https://github.com/discordjs/guide/blob/main/guide/preparations/setting-up-a-linter.md.
export default defineConfig(
	{ ignores: ['target/**', 'node_modules/**', 'web/dist/**', 'web/src/generated/**'] },
	js.configs.recommended,
	...tseslint.configs.recommended,
	{
		plugins: { '@stylistic': stylistic },
		languageOptions: { ecmaVersion: 'latest' },
		rules: {
			'@stylistic/arrow-spacing': ['warn', { before: true, after: true }],
			'@stylistic/brace-style': ['error', 'stroustrup', { allowSingleLine: true }],
			'@stylistic/comma-dangle': ['error', 'always-multiline'],
			'@stylistic/comma-spacing': 'error',
			'@stylistic/comma-style': 'error',
			curly: ['error', 'multi-line', 'consistent'],
			'@stylistic/dot-location': ['error', 'property'],
			'handle-callback-err': 'off',
			'@stylistic/indent': ['error', 'tab'],
			'@stylistic/keyword-spacing': 'error',
			'max-nested-callbacks': ['error', { max: 4 }],
			'max-statements-per-line': ['error', { max: 2 }],
			'no-console': 'off',
			'no-empty-function': 'error',
			'@stylistic/no-floating-decimal': 'error',
			'no-inline-comments': 'error',
			'no-lonely-if': 'error',
			'@stylistic/no-multi-spaces': 'error',
			'@stylistic/no-multiple-empty-lines': ['error', { max: 2, maxEOF: 1, maxBOF: 0 }],
			'no-shadow': 'off',
			'@typescript-eslint/no-shadow': ['error', { allow: ['err', 'resolve', 'reject'] }],
			'@stylistic/no-trailing-spaces': 'error',
			'no-var': 'error',
			'no-undef': 'off',
			'@stylistic/object-curly-spacing': ['error', 'always'],
			'prefer-const': 'error',
			'@stylistic/quotes': ['error', 'single'],
			'@stylistic/semi': ['error', 'always'],
			'@stylistic/space-before-blocks': 'error',
			'@stylistic/space-before-function-paren': [
				'error',
				{
					anonymous: 'never',
					named: 'never',
					asyncArrow: 'always',
				},
			],
			'@stylistic/space-in-parens': 'error',
			'@stylistic/space-infix-ops': 'error',
			'@stylistic/space-unary-ops': 'error',
			'@stylistic/spaced-comment': 'error',
			yoda: 'error',
		},
	},
	{
		files: ['**/*.{ts,tsx}', 'eslint.config.js'],
		languageOptions: {
			parserOptions: {
				projectService: { allowDefaultProject: ['eslint.config.js'] },
				tsconfigRootDir: import.meta.dirname,
			},
		},
		rules: {
			'@typescript-eslint/no-deprecated': 'error',
		},
	},
	{
		files: ['web/src/**/*.{ts,tsx}'],
		plugins: { 'react-hooks': reactHooks },
		rules: {
			'react-hooks/rules-of-hooks': 'error',
			'react-hooks/exhaustive-deps': 'error',
		},
	},
);
