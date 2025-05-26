"use client"
import { useEffect, useState } from 'react';
import { core } from "@tauri-apps/api";
import { AlertCircle, CheckCircle, Cpu, Wifi, Bluetooth, Usb, ArrowRight, RefreshCw, X } from "lucide-react";

export default function Home() {
  const [ports, setPorts] = useState<string[]>([]);
  const [selectedPort, setSelectedPort] = useState('');
  const [output, setOutput] = useState('');
  const [firmwareType, setFirmwareType] = useState('');
  const [showPopover, setShowPopover] = useState(false);
  const [isFlashing, setIsFlashing] = useState(false);
  const [flashStatus, setFlashStatus] = useState(''); // 'success', 'error', or ''
  const [isRefreshing, setIsRefreshing] = useState(false);

  // Poll for serial ports at regular intervals
  useEffect(() => {
    const pollInterval = setInterval(() => {
      refreshPorts();
    }, 3000); // Poll every 3 seconds

    // Initial load
    refreshPorts();

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

  const handleFirmwareTypeChange = (type:string) => {
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
    switch(firmwareType) {
      case "BLE":
        file = "/NPG-LITE-BLE.ino.bin";
        break;
      case "Serial":
        file = "/NPG-LITE.ino.bin";
        break;
      case "WiFi":
        file = "/NPG-LITE-WiFi.ino.bin";
        break;
    }
    
    setIsFlashing(true);
    setOutput("Flashing firmware, please wait...");
    setFlashStatus('');
    
    try {
      const result = await core.invoke("flash_firmware", {
        portname: selectedPort,
        file: file
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

  const closePopover = () => {
    setShowPopover(false);
    setFirmwareType('');
    setOutput('');
    setFlashStatus('');
  };

  const getFirmwareIcon = (type:string) => {
    switch(type) {
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
    <div className="min-h-screen bg-[#000409]">
      <div className="max-w-4xl mx-auto p-6">
        <header className="mb-8">
          <h1 className="text-3xl font-bold text-white-900">NPG-Lite-C6 Firmware Flasher</h1>
          <p className="text-white-600 mt-2">Select a firmware type to begin flashing your device</p>
        </header>

        <div className="bg-[#0C1117] rounded-lg shadow-md p-6 mb-6">
          <h2 className="text-xl font-semibold mb-4">Select Firmware Type</h2>
          
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
                  firmwareType === option.type ? 'border-blue-500 ring-2 ring-blue-200' : 'border-gray-200'
                }`}
              >
                <div className="flex items-center">
                  {getFirmwareIcon(option.type)}
                  <span className="font-medium">{option.title}</span>
                </div>
                <p className="text-gray-600 text-sm mt-2">{option.description}</p>
              </div>
            ))}
          </div>
        </div>

        {showPopover && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white rounded-lg shadow-xl max-w-md w-full">
              <div className="border-b p-4 flex justify-between items-center">
                <div className="flex text-black items-center">
                  {getFirmwareIcon(firmwareType)}
                  <h2 className="text-xl font-semibold">Flash {firmwareType} Firmware</h2>
                </div>
                <button 
                  onClick={closePopover}
                  className="text-gray-500 hover:text-gray-700"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>

              <div className="p-6 text-black">
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Select Serial Port
                  </label>
                  <div className="flex">
                    <select
                      onChange={e => setSelectedPort(e.target.value)}
                      value={selectedPort}
                      className="flex-grow border rounded-l-md p-2 focus:ring-blue-500 focus:border-blue-500"
                      disabled={isFlashing}
                    >
                      <option value="">-- Select Port --</option>
                      {ports.map((p) => <option key={p} value={p}>{p}</option>)}
                    </select>
                    <button 
                      onClick={refreshPorts} 
                      className="bg-gray-100 border border-l-0 rounded-r-md px-3 hover:bg-gray-200"
                      disabled={isFlashing || isRefreshing}
                    >
                      <RefreshCw className={`h-5 w-5 ${isRefreshing ? 'animate-spin' : ''}`} />
                    </button>
                  </div>
                </div>

                {output && (
                  <div className={`border rounded-md p-3 mt-4 ${getStatusColor()}`}>
                    <div className="flex items-start">
                      {getStatusIcon()}
                      <pre className="ml-2 text-sm whitespace-pre-wrap font-mono">{output}</pre>
                    </div>
                  </div>
                )}

                <div className="flex justify-end space-x-3 mt-6">
                  <button
                    onClick={closePopover}
                    className="px-4 py-2 border rounded-md text-gray-700 hover:bg-gray-50"
                    disabled={isFlashing}
                  >
                    Cancel
                  </button>
                  
                  <button
                    onClick={handleFlash}
                    disabled={isFlashing || !selectedPort}
                    className={`px-4 py-2 rounded-md flex items-center ${
                      !selectedPort 
                        ? 'bg-blue-300 cursor-not-allowed' 
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
      </div>
    </div>
  );
}