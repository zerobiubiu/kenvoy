'use strict';

const { selectTarget } = require('./config.cjs');
const { reuseBinary, writeBinary } = require('./files.cjs');
const { download } = require('./download.cjs');
const { extractBinary } = require('./archive.cjs');

async function install(packageRoot, options = {}) {
  const selected = selectTarget(packageRoot, options.platform, options.arch);
  const unix = !selected.key.startsWith('win32-');
  if (reuseBinary(selected.root, selected.target, unix)) return selected.binaryPath;
  const archive = await download(selected.target, options);
  const binary = extractBinary(archive, selected.target);
  return writeBinary(selected.root, selected.target, binary, unix);
}

module.exports = { install };
