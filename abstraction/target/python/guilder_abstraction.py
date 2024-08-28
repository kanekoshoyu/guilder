from abc import ABC, abstractmethod

class TestServer(ABC):
	"""test server network connection"""
	@abstractmethod
	def ping(self) -> bool:
		"""test ping"""
		pass

	@abstractmethod
	def get_server_time(self) -> int:
		"""get server local time"""
		pass


class GetMarketData(ABC):
	"""get market data such as symbol, price and volume"""
	@abstractmethod
	def get_symbol(self) -> list[str]:
		"""get symbol, such as BTCUSD"""
		pass

	@abstractmethod
	def get_price(self, symbol: str) -> float:
		"""get mid-price of a symbol (e.g. BTCUSD -> 67000.0)"""
		pass

	@abstractmethod
	def get_orderbook(self, symbol: str) -> dict[float, float]:
		"""get orderbook"""
		pass


class ManageOrder(ABC):
	"""place, change, cancel order"""
	@abstractmethod
	def place_order(self, symbol: str, price: int, volume: int) -> int:
		"""place order, return cloid"""
		pass

	@abstractmethod
	def change_order_by_cloid(self, cloid: int, price: int, volume: int) -> int:
		"""change order"""
		pass

	@abstractmethod
	def cancel_order(self, cloid: int) -> int:
		"""cancel order by cloid"""
		pass

	@abstractmethod
	def cancel_all_order(self) -> bool:
		"""cancel all order regardless of cloid/symbol"""
		pass


