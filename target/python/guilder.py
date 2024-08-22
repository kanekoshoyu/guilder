class TestServer:
    def ping(self) -> bool:
        pass

    def get_server_time(self) -> int:
        pass


class GetMarketData:
    def get_symbol(self, x: str, y: int) -> list[str]:
        pass

    def get_price(self, x: str, y: int) -> float:
        pass


class ManageOrder:
    def place_order(self, x: str, price: int, volume: int) -> int:
        pass

    def change_order(self, x: str, price: int, volume: int) -> int:
        pass

    def cancel_order(self, x: str) -> int:
        pass


