class GetMarketData:
    def get_price(self, x: str, y: int) -> int:
        pass


class OrderPlace:
    def get_price(self, x: str, price: int, volume: int) -> int:
        pass


class OrderChange:
    def get_price(self, x: str, price: int, volume: int) -> int:
        pass


class OrderCancel:
    def get_price(self, x: str) -> int:
        pass


