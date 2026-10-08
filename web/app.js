import init, { start } from './benchy_client.js';

const params = new URLSearchParams(location.search);
const name = params.get('name') || 'player';

await init();
start(name);
