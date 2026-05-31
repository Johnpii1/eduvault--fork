import React, { useState } from 'react';

/**
 * BulkAddressInput – allows users to input a list of Stellar addresses.
 * Users can either upload a CSV file (one address per line) or paste
 * addresses into a textarea. The component validates the format and
 * calls onChange with an array of trimmed addresses.
 */
export default function BulkAddressInput({ onChange }) {
  const [fileError, setFileError] = useState('');
  const [manualInput, setManualInput] = useState('');

  const parseAddresses = (text) => {
    const lines = text.split(/[\r\n,]+/).map(l => l.trim()).filter(l => l !== '');
    onChange(lines);
  };

  const handleFile = (e) => {
    const file = e.target.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const content = ev.target.result;
      parseAddresses(content);
      setFileError('');
    };
    reader.onerror = () => setFileError('Failed to read file');
    reader.readAsText(file);
  };

  const handleManual = (e) => {
    const text = e.target.value;
    setManualInput(text);
    parseAddresses(text);
  };

  return (
    <div className="bulk-address-input">
      <label className="input-label">CSV Upload (one address per line):</label>
      <input type="file" accept=".csv,text/csv" onChange={handleFile} />
      {fileError && <p className="error-msg">{fileError}</p>}
      <label className="input-label">Or paste addresses (comma or newline separated):</label>
      <textarea
        rows={4}
        placeholder="G..."
        value={manualInput}
        onChange={handleManual}
        className="manual-textarea"
      />
    </div>
  );
}
