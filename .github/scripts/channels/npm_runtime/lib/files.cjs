'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createHash, randomUUID } = require('node:crypto');

function statIfPresent(filename) {
  try {
    return fs.lstatSync(filename);
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

function cacheDirectory(root, target, create = false) {
  let directory = root;
  for (const segment of ['vendor', target.binarySha256]) {
    directory = path.join(directory, segment);
    let stat = statIfPresent(directory);
    if (!stat && create) {
      try {
        fs.mkdirSync(directory);
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
      }
      stat = statIfPresent(directory);
    }
    if (stat && (!stat.isDirectory() || stat.isSymbolicLink() || fs.realpathSync(directory) !== directory)) {
      throw new Error('vendor 及摘要缓存必须是包内的真实目录，不能是链接。');
    }
    if (!stat && create) throw new Error('无法创建 Kenvoy CLI 摘要缓存目录。');
  }
  return directory;
}

function installedPath(root, target) {
  const directory = cacheDirectory(root, target);
  const filename = path.join(directory, target.executable);
  const stat = statIfPresent(filename);
  if (stat && (!stat.isFile() || stat.isSymbolicLink())) {
    throw new Error('已安装的 Kenvoy CLI 必须是普通文件，不能是链接或目录。');
  }
  return { filename, stat };
}

function reuseBinary(root, target, unix) {
  const { filename, stat } = installedPath(root, target);
  if (!stat || stat.size !== target.binarySize) return false;
  const fd = fs.openSync(filename, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
  try {
    const opened = fs.fstatSync(fd);
    if (!opened.isFile() || opened.dev !== stat.dev || opened.ino !== stat.ino) {
      throw new Error('已安装文件在校验期间被替换。');
    }
    const hash = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let length;
    while ((length = fs.readSync(fd, buffer, 0, buffer.length, null)) !== 0) {
      hash.update(buffer.subarray(0, length));
    }
    if (hash.digest('hex') !== target.binarySha256) return false;
    const current = installedPath(root, target).stat;
    if (!current || current.dev !== opened.dev || current.ino !== opened.ino || current.size !== target.binarySize) {
      throw new Error('已安装文件在校验期间被替换。');
    }
    if (unix && (opened.mode & 0o7777) !== 0o755) fs.fchmodSync(fd, 0o755);
    return true;
  } finally {
    fs.closeSync(fd);
  }
}

function writeBinary(root, target, binary, unix) {
  const directory = cacheDirectory(root, target, true);
  const { filename } = installedPath(root, target);
  const temporary = path.join(directory, `.${target.executable}.${process.pid}.${randomUUID()}.tmp`);
  let fd;
  let created = false;
  try {
    fd = fs.openSync(temporary, fs.constants.O_WRONLY | fs.constants.O_CREAT
      | fs.constants.O_EXCL | (fs.constants.O_NOFOLLOW || 0), unix ? 0o755 : 0o644);
    created = true;
    fs.writeFileSync(fd, binary);
    if (unix) fs.fchmodSync(fd, 0o755);
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = undefined;
    installedPath(root, target);
    // 同目录 rename：成功才替换旧文件，失败不先删除旧文件。
    fs.renameSync(temporary, filename);
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
    if (created) {
      try {
        fs.unlinkSync(temporary);
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
  }
  return filename;
}

module.exports = { installedPath, reuseBinary, writeBinary };
