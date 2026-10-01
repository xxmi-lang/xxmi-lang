/** Entry of the `xxmi` single-file binary. */
import { main } from '../../packages/cli/src/cli.ts';
import { setupEmbedded } from './embedded.ts';

await setupEmbedded();
process.exitCode = await main(process.argv.slice(2));
