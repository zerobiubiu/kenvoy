'use strict';

const { createHash } = require('node:crypto');
const { unzipSync } = require('fflate');
const { assertMemberPath } = require('./config.cjs');

function assertDigest(bytes, expected, label) {
  if (createHash('sha256').update(bytes).digest('hex') !== expected) {
    throw new Error(`${label} SHA-256 校验失败。`);
  }
}

function extractBinary(bytes, target) {
  if (bytes.length !== target.archiveSize) {
    throw new Error('ZIP 下载长度与清单不一致。');
  }
  assertDigest(bytes, target.archiveSha256, 'ZIP');
  const names = new Set();
  let matches = 0;
  let files;
  try {
    files = unzipSync(bytes, {
      filter(entry) {
        assertMemberPath(entry.name, true);
        if (names.has(entry.name)) {
          throw new Error('ZIP 含重复成员。');
        }
        names.add(entry.name);
        if (entry.name.endsWith('/') || entry.name.split('/').at(-1) !== target.executable) {
          return false;
        }
        matches += 1;
        if (matches > 1 || entry.name !== target.member) {
          throw new Error('ZIP 中的可执行成员不唯一或与清单不一致。');
        }
        if (entry.originalSize !== target.binarySize) {
          throw new Error('ZIP 成员长度与清单不一致。');
        }
        return true;
      },
    });
  } catch (error) {
    if (error.message.startsWith('ZIP ')) throw error;
    throw new Error('ZIP 归档损坏或使用了不支持的压缩格式。');
  }
  if (matches !== 1 || !Object.hasOwn(files, target.member)) {
    throw new Error('ZIP 中缺少清单指定的可执行成员。');
  }
  const binary = files[target.member];
  if (binary.length !== target.binarySize) {
    throw new Error('二进制长度与清单不一致。');
  }
  assertDigest(binary, target.binarySha256, '二进制');
  return binary;
}

module.exports = { extractBinary };
