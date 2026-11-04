'use strict';

const { install } = require('./lib/install.cjs');

install(__dirname).catch((error) => {
  console.error(`kenvoy 安装失败：${error.message}`);
  process.exitCode = 1;
});
