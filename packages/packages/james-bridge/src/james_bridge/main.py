"""JAMES Bridge - Main entry point"""

from .client import main

if __name__ == "__main__":
    import asyncio
    asyncio.run(main())