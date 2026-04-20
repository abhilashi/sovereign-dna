const fs = require('fs');
const glob = require('glob');

// This is a naive script to find buttons without aria-labels or text content
const files = glob.sync('src/**/*.tsx');
files.forEach(file => {
  const content = fs.readFileSync(file, 'utf-8');
  const lines = content.split('\n');
  lines.forEach((line, index) => {
    if (line.includes('<button')) {
      console.log(`Found button in ${file}:${index + 1}`);
    }
  });
});
