import { writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import openapiTS, { astToString } from 'openapi-typescript';
import { format, resolveConfig } from 'prettier';
import ts from 'typescript';

// Export the schema from the local Rust backend
const schema = execFileSync('cargo', ['run', '--quiet', '--locked', '--', '--openapi'], {
	encoding: 'utf8',
	stdio: ['ignore', 'pipe', 'inherit'],
});
const output = new URL('../web/src/generated/api.ts', import.meta.url);
const outputPath = fileURLToPath(output);

// Convert the schema into TypeScript
const ast = await openapiTS(JSON.parse(schema), {
	transform(schemaObject) {
		if (schemaObject.format === 'binary') {
			return ts.factory.createTypeReferenceNode('Blob');
		}
	},
});

const content = await format(
	'/** Generated from the Rust OpenAPI schema by `npm run generate:api`. Do not edit. */\n' +
		astToString(ast),
	{ ...(await resolveConfig(outputPath)), filepath: outputPath },
);

await mkdir(new URL('.', output), { recursive: true });
await writeFile(output, content);
