/**
 * ClearMemoryAI — Electron main process icon setup
 * Add to your main.js where you create the BrowserWindow.
 */

const path = require('path');

const mainWindow = new BrowserWindow({
  width: 1200,
  height: 800,
  title: 'ClearMemoryAI',
  icon: path.join(__dirname, 'assets', 'icons',
    process.platform === 'win32' ? 'favicon.ico' : 'icon-512.png'
  ),
  webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    nodeIntegration: false,
    contextIsolation: true,
  },
});

const { app } = require('electron');
app.setName('ClearMemoryAI');
