/** Entry of the `xxmi-lsp` single-file binary. */
import { main } from '../../packages/lsp/src/main.ts';
import { setupEmbedded } from './embedded.ts';

await setupEmbedded();
main(process.argv.slice(2));
