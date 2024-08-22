from abc import ABC, abstractmethod

class TestServer(ABC):
    @abstractmethod
    def ping(self) -> bool:
        pass

    @abstractmethod
    def get_server_time(self) -> int:
        pass


class GetMarketData(ABC):
    @abstractmethod
    def get_symbol(self) -> list[str]:
        pass

    @abstractmethod
    def get_price(self, symbol: str) -> float:
        pass


class ManageOrder(ABC):
    @abstractmethod
    def place_order(self, x: str, price: int, volume: int) -> int:
        pass

    @abstractmethod
    def change_order(self, x: str, price: int, volume: int) -> int:
        pass

    @abstractmethod
    def cancel_order(self, x: str) -> int:
        pass


