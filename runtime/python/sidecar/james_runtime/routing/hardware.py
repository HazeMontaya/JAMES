"""JAMES Runtime Hardware Detection"""
import logging
import subprocess
import platform
import psutil
from pydantic import BaseModel
from typing import Optional, List

logger = logging.getLogger(__name__)


class HardwareProfile(BaseModel):
    gpu_name: str = "Unknown"
    vram_gb: float = 0.0
    cuda_version: Optional[str] = None
    rocm_version: Optional[str] = None
    metal_support: bool = False
    cpu_cores: int = 0
    ram_gb: float = 0.0
    nvlink: bool = False
    pcie_gen: int = 0
    driver_version: str = ""
    platform: str = ""
    architecture: str = ""


class HardwareDetector:
    """Detect system hardware capabilities"""
    
    def __init__(self):
        self._cached_profile: Optional[HardwareProfile] = None
    
    def detect(self) -> HardwareProfile:
        if self._cached_profile:
            return self._cached_profile
        
        profile = HardwareProfile(
            gpu_name="Unknown",
            vram_gb=0.0,
            cuda_version=None,
            rocm_version=None,
            metal_support=False,
            cpu_cores=psutil.cpu_count(logical=False) or 1,
            ram_gb=psutil.virtual_memory().total / (1024**3),
            platform=platform.system(),
            architecture=platform.machine(),
        )
        
        # Detect GPU
        self._detect_gpu(profile)
        
        self._cached_profile = profile
        logger.info(f"Hardware detected: {profile.gpu_name}, VRAM: {profile.vram_gb:.1f}GB, CPU: {profile.cpu_cores} cores, RAM: {profile.ram_gb:.1f}GB")
        return profile
    
    def _detect_gpu(self, profile: HardwareProfile) -> None:
        system = platform.system()
        
        if system == "Windows":
            self._detect_windows_gpu(profile)
        elif system == "Linux":
            self._detect_linux_gpu(profile)
        elif system == "Darwin":
            self._detect_macos_gpu(profile)
        
        # Try GPUtil as fallback
        if profile.vram_gb == 0:
            self._detect_with_gputil(profile)
    
    def _detect_windows_gpu(self, profile: HardwareProfile) -> None:
        try:
            # nvidia-smi
            result = subprocess.run(
                ["nvidia-smi", "--query-gpu=name,memory.total,driver_version", "--format=csv,noheader,nounits"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                lines = result.stdout.strip().split('\n')
                if lines:
                    name, vram_mb, driver = lines[0].split(', ')
                    profile.gpu_name = name.strip()
                    profile.vram_gb = int(vram_mb.strip()) / 1024
                    profile.driver_version = driver.strip()
                    profile.cuda_version = self._get_cuda_version()
                    return
        except Exception:
            pass
        
        try:
            # rocm-smi for AMD
            result = subprocess.run(
                ["rocm-smi", "--showproductname", "--showvram", "--showdriverversion"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                # Parse ROCm output
                profile.rocm_version = self._get_rocm_version()
        except Exception:
            pass
    
    def _detect_linux_gpu(self, profile: HardwareProfile) -> None:
        try:
            result = subprocess.run(
                ["nvidia-smi", "--query-gpu=name,memory.total,driver_version", "--format=csv,noheader,nounits"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                lines = result.stdout.strip().split('\n')
                if lines:
                    name, vram_mb, driver = lines[0].split(', ')
                    profile.gpu_name = name.strip()
                    profile.vram_gb = int(vram_mb.strip()) / 1024
                    profile.driver_version = driver.strip()
                    profile.cuda_version = self._get_cuda_version()
                    return
        except Exception:
            pass
        
        try:
            result = subprocess.run(
                ["rocm-smi", "--showproductname", "--showvram", "--showdriverversion"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                profile.rocm_version = self._get_rocm_version()
        except Exception:
            pass
    
    def _detect_macos_gpu(self, profile: HardwareProfile) -> None:
        profile.metal_support = True
        try:
            result = subprocess.run(
                ["system_profiler", "SPDisplaysDataType"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                # Parse for GPU name
                for line in result.stdout.split('\n'):
                    if 'Chipset Model' in line or 'GPU' in line:
                        profile.gpu_name = line.split(':')[-1].strip()
                        break
        except Exception:
            pass
    
    def _detect_with_gputil(self, profile: HardwareProfile) -> None:
        try:
            import GPUtil
            gpus = GPUtil.getGPUs()
            if gpus:
                gpu = gpus[0]
                profile.gpu_name = gpu.name
                profile.vram_gb = gpu.memoryTotal / 1024
        except Exception:
            pass
    
    def _get_cuda_version(self) -> Optional[str]:
        try:
            result = subprocess.run(
                ["nvcc", "--version"],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                for line in result.stdout.split('\n'):
                    if 'release' in line:
                        return line.split('release')[-1].split(',')[0].strip()
        except Exception:
            pass
        return None
    
    def _get_rocm_version(self) -> Optional[str]:
        try:
            result = subprocess.run(
                ["rocminfo", "--version"],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                return result.stdout.strip()
        except Exception:
            pass
        return None