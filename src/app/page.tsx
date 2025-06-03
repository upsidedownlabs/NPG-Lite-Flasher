"use client"
import { useEffect, useState } from 'react';
import { core } from "@tauri-apps/api";
import { AlertCircle, CheckCircle, Cpu, Wifi, Bluetooth, Usb, ArrowRight, RefreshCw, X, Plus, Upload, Trash2, Download, Github } from "lucide-react";
type FirmwareFile = [string, string]; // [url, name]

export default function Home() {
  const [ports, setPorts] = useState<string[]>([]);
  const [selectedPort, setSelectedPort] = useState('');
  const [output, setOutput] = useState('');
  const [firmwareType, setFirmwareType] = useState('');
  const [showPopover, setShowPopover] = useState(false);
  const [isFlashing, setIsFlashing] = useState(false);
  const [flashStatus, setFlashStatus] = useState(''); // 'success', 'error', or ''
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [customFirmwares, setCustomFirmwares] = useState<string[]>([]);
  const [showUploadDialog, setShowUploadDialog] = useState(false);
  const [newFirmwareName, setNewFirmwareName] = useState('');
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [showGithubDialog, setShowGithubDialog] = useState(false);
  const [githubFirmwares, setGithubFirmwares] = useState<FirmwareFile[]>([]);
  const [isFetchingGithub, setIsFetchingGithub] = useState(false);
  const [githubRepo, setGithubRepo] = useState('upsidedownlabs/npg-lite-firmware');

  // Poll for serial ports at regular intervals
  useEffect(() => {
    const pollInterval = setInterval(() => {
      refreshPorts();
    }, 3000); // Poll every 3 seconds

    // Initial load
    refreshPorts();
    loadCustomFirmwares();

    return () => clearInterval(pollInterval); // Cleanup on unmount
  }, []);

  const refreshPorts = async () => {
    setIsRefreshing(true);
    try {
      core.invoke<string[]>("list_serial_ports").then(setPorts);
    } catch (err) {
      console.error("Error listing ports:", err);
    } finally {
      setIsRefreshing(false);
    }
  };

  // Update the loadCustomFirmwares function
  const loadCustomFirmwares = async () => {
    try {
      const firmwares = await core.invoke<string[]>("list_custom_firmwares");
      setCustomFirmwares(firmwares);
    } catch (err) {
      console.error("Error loading custom firmwares:", err);
    }
  };

  const fetchGithubReleases = async () => {
    setIsFetchingGithub(true);
    try {
      const releases = await core.invoke<FirmwareFile[]>("fetch_github_releases", {
        repo: githubRepo
      });
      setGithubFirmwares(releases);
      setShowGithubDialog(true);
    } catch (err) {
      console.error("Error fetching GitHub releases:", err);
      setOutput(`Error fetching releases: ${String(err)}`);
      setFlashStatus('error');
    } finally {
      setIsFetchingGithub(false);
    }
  };


  ////
  // Update the handleGithubFirmwareSelect function
  const handleGithubFirmwareSelect = async (url: string, name: string) => {
    try {
      console.log("Invoking download_and_save_firmware with:", { url, name });

      const downloadedName = await core.invoke<string>("download_and_save_firmware", {
        url,
        name
      });

      await loadCustomFirmwares();
      setShowGithubDialog(false);
      handleFirmwareTypeChange(downloadedName);
    } catch (err) {
      console.error("Error downloading firmware:", err);
      setOutput(`Error downloading firmware: ${String(err)}`);
      setFlashStatus('error');
    }
  };

  const handleFirmwareTypeChange = (type: string) => {
    setFirmwareType(type);
    if (type) {
      setShowPopover(true);
      setFlashStatus('');
      setOutput('');
    }
  };

  const handleFlash = async () => {
    if (!selectedPort) {
      setOutput("Error: Please select a port first.");
      setFlashStatus('error');
      return;
    }

    let file = "";
    let isCustom = false;

    switch (firmwareType) {
      case "BLE":
        file = "/NPG-LITE-BLE.ino.bin";
        break;
      case "Serial":
        file = "/NPG-LITE.ino.bin";
        break;
      case "WiFi":
        file = "/NPG-LITE-WiFi.ino.bin";
        break;
      default:
        // Handle custom firmware
        file = firmwareType;
        isCustom = true;
    }

    setIsFlashing(true);
    setOutput("Flashing firmware, please wait...");
    setFlashStatus('');

    try {
      const result = await core.invoke("flash_firmware", {
        portname: selectedPort,
        file: file,
        iscustom: isCustom
      });
      setOutput(`Success: ${String(result)}`);
      setFlashStatus('success');
    } catch (err) {
      console.error("Flash failed:", err);
      setOutput(`Error: ${String(err)}`);
      setFlashStatus('error');
    } finally {
      setIsFlashing(false);
    }
  };


  // Update the handleFileUpload function
  const handleFileUpload = async () => {
    if (!selectedFile || !newFirmwareName) {
      alert("Please select a file and enter a name");
      return;
    }

    setIsUploading(true);
    try {
      await loadCustomFirmwares();
      setShowUploadDialog(false);
      setNewFirmwareName('');
      setSelectedFile(null);
    } catch (err) {
      console.error("Upload failed:", err);
      alert(`Failed to upload firmware: ${err}`);
    } finally {
      setIsUploading(false);
    }
  };


  const handleDeleteFirmware = async (filename: string) => {
    if (!confirm(`Are you sure you want to delete ${filename}?`)) {
      return;
    }

    try {
      await core.invoke<void>("delete_custom_firmware", { filename });
      await loadCustomFirmwares();
    } catch (err) {
      console.error("Error deleting firmware:", err);
      alert(`Failed to delete firmware: ${err}`);
    }
  };
  const closePopover = () => {
    setShowPopover(false);
    setFirmwareType('');
    setOutput('');
    setFlashStatus('');
  };

  const getFirmwareIcon = (type: string) => {
    switch (type) {
      case "BLE":
        return <Bluetooth className="mr-2" />;
      case "Serial":
        return <Usb className="mr-2" />;
      case "WiFi":
        return <Wifi className="mr-2" />;
      default:
        return <Cpu className="mr-2" />;
    }
  };

  const getStatusColor = () => {
    if (flashStatus === 'success') return 'bg-green-50 border-green-200 text-green-800';
    if (flashStatus === 'error') return 'bg-red-50 border-red-200 text-red-800';
    return 'bg-gray-50 border-gray-200 text-gray-800';
  };

  const getStatusIcon = () => {
    if (flashStatus === 'success') return <CheckCircle className="h-5 w-5 text-green-500" />;
    if (flashStatus === 'error') return <AlertCircle className="h-5 w-5 text-red-500" />;
    return null;
  };

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900">
      <div className="max-w-4xl mx-auto p-6">
        <header className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900 dark:text-white">NPG-Lite-C6 Firmware Flasher</h1>
          <p className="text-gray-600 dark:text-gray-400 mt-2">Select a firmware type to begin flashing your device</p>
        </header>

        <div className="bg-white dark:bg-gray-800 rounded-lg shadow-md border border-gray-200 dark:border-gray-700 p-6 mb-6">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold text-gray-900 dark:text-white">Select Firmware Type</h2>
            <div className="flex gap-2">
              <button
                onClick={fetchGithubReleases}
                disabled={isFetchingGithub}
                className="flex items-center gap-2 px-3 py-1 bg-gray-600 hover:bg-gray-700 dark:bg-gray-500 dark:hover:bg-gray-600 text-white rounded-md text-sm transition-colors"
              >
                {isFetchingGithub ? (
                  <RefreshCw className="animate-spin h-4 w-4" />
                ) : (
                  <>
                    <Github className="h-4 w-4" /> Get from GitHub
                  </>
                )}
              </button>
              <button
                onClick={() => setShowUploadDialog(true)}
                className="flex items-center gap-2 px-3 py-1 bg-blue-600 hover:bg-blue-700 text-white rounded-md text-sm transition-colors"
              >
                <Plus size={16} /> Add Custom
              </button>
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {[
              { type: "BLE", title: "Bluetooth LE", description: "For wireless Bluetooth connectivity" },
              { type: "Serial", title: "Serial", description: "For wired USB serial connections" },
              { type: "WiFi", title: "WiFi", description: "For wireless network connectivity" }
            ].map((option) => (
              <div
                key={option.type}
                onClick={() => handleFirmwareTypeChange(option.type)}
                className={`border rounded-lg p-4 cursor-pointer transition-all duration-200 hover:shadow-md ${
                  firmwareType === option.type
                    ? 'border-blue-500 ring-2 ring-blue-200 dark:ring-blue-400/30 bg-blue-50 dark:bg-blue-900/20'
                    : 'border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 hover:bg-gray-50 dark:hover:bg-gray-600'
                }`}
              >
                <div className="flex items-center text-gray-900 dark:text-white">
                  {getFirmwareIcon(option.type)}
                  <span className="font-medium">{option.title}</span>
                </div>
                <p className="text-gray-600 dark:text-gray-400 text-sm mt-2">{option.description}</p>
              </div>
            ))}

            {customFirmwares.length > 0 && (
              <>
                <div className="col-span-full border-t border-gray-300 dark:border-gray-600 my-4"></div>
                <h3 className="col-span-full text-lg font-medium text-gray-900 dark:text-white">Custom Firmwares</h3>
                {customFirmwares.map((fw) => (
                  <div
                    key={fw}
                    className={`border rounded-lg p-4 cursor-pointer transition-all duration-200 hover:shadow-md ${
                      firmwareType === fw
                        ? 'border-blue-500 ring-2 ring-blue-200 dark:ring-blue-400/30 bg-blue-50 dark:bg-blue-900/20'
                        : 'border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 hover:bg-gray-50 dark:hover:bg-gray-600'
                    }`}
                  >
                    <div className="flex justify-between items-center">
                      <div
                        className="flex items-center w-full text-gray-900 dark:text-white"
                        onClick={() => handleFirmwareTypeChange(fw)}
                      >
                        <Cpu className="mr-2" />
                        <span className="font-medium">{fw.slice(0, -4)}</span>
                      </div>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDeleteFirmware(fw);
                        }}
                        className="text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 p-1 transition-colors"
                        title="Delete firmware"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                    <p className="text-gray-600 dark:text-gray-400 text-sm mt-2">Custom firmware</p>
                  </div>
                ))}
              </>
            )}
          </div>
        </div>

        {showPopover && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full border border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 dark:border-gray-700 p-4 flex justify-between items-center">
                <div className="flex text-gray-900 dark:text-white items-center">
                  {getFirmwareIcon(firmwareType)}
                  <h2 className="text-xl font-semibold">
                    Flash {["BLE", "Serial", "WiFi"].includes(firmwareType) ? firmwareType : "Custom"} Firmware
                  </h2>
                </div>
                <button
                  onClick={closePopover}
                  className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>

              <div className="p-6 text-gray-900 dark:text-white">
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Select Serial Port
                  </label>
                  <div className="flex">
                    <select
                      onChange={e => setSelectedPort(e.target.value)}
                      value={selectedPort}
                      className="flex-grow border border-gray-300 dark:border-gray-600 rounded-l-md p-2 focus:ring-blue-500 focus:border-blue-500 bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                      disabled={isFlashing}
                    >
                      <option value="">-- Select Port --</option>
                      {ports.map((p) => <option key={p} value={p}>{p}</option>)}
                    </select>
                    <button
                      onClick={refreshPorts}
                      className="bg-gray-100 dark:bg-gray-700 border border-l-0 border-gray-300 dark:border-gray-600 rounded-r-md px-3 hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
                      disabled={isFlashing || isRefreshing}
                    >
                      <RefreshCw className={`h-5 w-5 text-gray-600 dark:text-gray-400 ${isRefreshing ? 'animate-spin' : ''}`} />
                    </button>
                  </div>
                </div>

                {output && (
                  <div className={`border rounded-md p-3 mt-4 ${getStatusColor()}`}>
                    <div className="flex items-start">
                      {getStatusIcon()}
                      <pre className="ml-2 text-sm whitespace-pre-wrap font-mono text-gray-800 dark:text-gray-200">{output}</pre>
                    </div>
                  </div>
                )}

                <div className="flex justify-end space-x-3 mt-6">
                  <button
                    onClick={closePopover}
                    className="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                    disabled={isFlashing}
                  >
                    Cancel
                  </button>

                  <button
                    onClick={handleFlash}
                    disabled={isFlashing || !selectedPort}
                    className={`px-4 py-2 rounded-md flex items-center transition-colors ${
                      !selectedPort
                        ? 'bg-blue-400 cursor-not-allowed'
                        : 'bg-blue-600 hover:bg-blue-700'
                    } text-white`}
                  >
                    {isFlashing ? (
                      <>
                        <RefreshCw className="animate-spin h-4 w-4 mr-2" />
                        Flashing...
                      </>
                    ) : (
                      <>
                        Flash <ArrowRight className="ml-2 h-4 w-4" />
                      </>
                    )}
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {showUploadDialog && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full border border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 dark:border-gray-700 p-4 flex justify-between items-center">
                <h2 className="text-xl font-semibold text-gray-900 dark:text-white">Upload Custom Firmware</h2>
                <button
                  onClick={() => setShowUploadDialog(false)}
                  className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>

              <div className="p-6 text-gray-900 dark:text-white">
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Firmware Name
                  </label>
                  <input
                    type="text"
                    value={newFirmwareName}
                    onChange={(e) => setNewFirmwareName(e.target.value)}
                    placeholder="e.g., MyCustomFirmware"
                    className="w-full border border-gray-300 dark:border-gray-600 rounded-md p-2 focus:ring-blue-500 focus:border-blue-500 bg-white dark:bg-gray-700 text-gray-900 dark:text-white placeholder-gray-500 dark:placeholder-gray-400"
                  />
                </div>

                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Firmware File (.bin)
                  </label>
                  <div className="flex items-center">
                    <label className="flex items-center px-4 py-2 bg-gray-100 dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-md cursor-pointer hover:bg-gray-200 dark:hover:bg-gray-600 text-gray-900 dark:text-white transition-colors">
                      <Upload className="h-5 w-5 mr-2" />
                      {selectedFile ? selectedFile.name : "Choose File"}
                      <input
                        type="file"
                        accept=".bin"
                        onChange={(e) => setSelectedFile(e.target.files?.[0] || null)}
                        className="hidden"
                      />
                    </label>
                  </div>
                </div>

                <div className="flex justify-end space-x-3 mt-6">
                  <button
                    onClick={() => setShowUploadDialog(false)}
                    className="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                    disabled={isUploading}
                  >
                    Cancel
                  </button>

                  <button
                    onClick={handleFileUpload}
                    disabled={isUploading || !selectedFile || !newFirmwareName}
                    className={`px-4 py-2 rounded-md flex items-center transition-colors ${
                      !selectedFile || !newFirmwareName
                        ? 'bg-blue-400 cursor-not-allowed'
                        : 'bg-blue-600 hover:bg-blue-700'
                    } text-white`}
                  >
                    {isUploading ? (
                      <>
                        <RefreshCw className="animate-spin h-4 w-4 mr-2" />
                        Uploading...
                      </>
                    ) : (
                      "Upload Firmware"
                    )}
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {showGithubDialog && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4 border border-gray-200 dark:border-gray-700 max-h-[90vh] flex flex-col">
              <div className="border-b border-gray-200 dark:border-gray-700 p-4 flex justify-between items-center flex-shrink-0">
                <div>
                  <h2 className="text-xl font-semibold text-gray-900 dark:text-white">Select Firmware from GitHub</h2>
                </div>
                <button
                  onClick={() => {
                    setShowGithubDialog(false);
                    setGithubRepo('Amanmahe/npg-lite-firmware');
                  }}
                  className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>

              <div className="p-6 text-gray-900 dark:text-white overflow-y-auto flex-1 min-h-0">
                {githubFirmwares.length === 0 ? (
                  <div className="text-center">
                    <p className="text-gray-600 dark:text-gray-400 mb-4">No firmware files found in the latest release.</p>
                    <button
                      onClick={fetchGithubReleases}
                      className="flex items-center gap-2 px-3 py-1 bg-gray-600 hover:bg-gray-700 dark:bg-gray-500 dark:hover:bg-gray-600 text-white rounded-md text-sm mx-auto transition-colors"
                    >
                      <RefreshCw className="h-4 w-4" /> Refresh
                    </button>
                  </div>
                ) : (
                  <div className="space-y-2">
                    {githubFirmwares.map((fws, index) => {
                      const [name, url] = fws;
                      return (
                        <div
                          key={index}
                          className="border border-gray-300 dark:border-gray-600 rounded-lg p-3 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700 flex justify-between items-center transition-colors"
                          onClick={() => handleGithubFirmwareSelect(url, name)}
                        >
                          <div className="flex items-center">
                            <Download className="h-5 w-5 mr-2 text-blue-600 dark:text-blue-400" />
                            <span className="text-gray-900 dark:text-white truncate">{name || 'Default Name'}</span>
                          </div>
                          <ArrowRight className="h-5 w-5 text-gray-400 dark:text-gray-500 flex-shrink-0 ml-2" />
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}