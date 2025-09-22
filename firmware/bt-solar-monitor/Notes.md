Startup 


AT
-> OK

AT+CGDCONT=1,"IP","gprs.swisscom.ch"
-> OK

AT+CREG?
-> +CREG: 0,1
-> Ok

AT+HTTPINIT
-> OK

AT+HTTPPARA="URL","http://api.solar.bockmattli.ch/api/v1/solar"
-> OK

AT+HTTPACTION=0
-> OK

AT+HTTPREAD?
-> +HTTPREAD: LEN,10
-> OK

AT+HTTPREAD=0,10
-> OK
-> +HTTPREAD: 10
-> solar data
-> +HTTPREAD: 0