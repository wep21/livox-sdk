// Test adapter compiled against unmodified official SDK2 sources.
#include "comm/sdk_protocol.h"
#include "livox_lidar_def.h"
#include <cassert>
#include <cstddef>
#include <cstring>
#include <fstream>
using namespace livox::lidar;
void save(const char* path,const void* b,size_t n) {std::ofstream(path,std::ios::binary).write(static_cast<const char*>(b),n);}
int main() {
  static_assert(sizeof(SdkPreamble)==24, "command header");
  static_assert(offsetof(LivoxLidarEthernetPacket,data)==36,"data header");
  static_assert(sizeof(LivoxLidarCartesianHighRawPoint)==14,"high point");
  static_assert(sizeof(LivoxLidarCartesianLowRawPoint)==8,"low point");
  static_assert(sizeof(LivoxLidarSpherPoint)==10,"spherical point");
  static_assert(sizeof(LivoxLidarImuRawPoint)==24,"imu");
  SdkProtocol codec; uint8_t buf[1400]={};uint32_t len=0;
  uint8_t payload[]={0,0xff,0x42,0x80,1};
  CommPacket packet={};packet.protocol=kLidarSdk;packet.seq_num=0x12345678;packet.cmd_id=0x100;packet.cmd_type=0;packet.sender_type=0;packet.data=payload;packet.data_len=sizeof(payload);
  assert(codec.Pack(buf,sizeof(buf),&len,packet)==0);assert(codec.CheckPreamble(buf,len));
  save("sdk2-command.bin",buf,len);
  CommPacket decoded={};assert(codec.ParsePacket(buf,len,&decoded));assert(decoded.seq_num==0x5678);
  packet.data_len=0;std::memset(buf,0,sizeof(buf));assert(codec.Pack(buf,sizeof(buf),&len,packet)==0);save("sdk2-command-empty.bin",buf,len);
  // Header/point structs come from the SDK; data CRC is the documented coverage.
  uint8_t data[60]={};auto* p=reinterpret_cast<LivoxLidarEthernetPacket*>(data);
  p->length=50;p->time_interval=1000;p->dot_num=1;p->udp_cnt=65535;p->frame_cnt=254;p->data_type=1;p->time_type=1;
  uint64_t t=1234567890123ULL;std::memcpy(p->timestamp,&t,8);
  LivoxLidarCartesianHighRawPoint high={-123456,234567,-345678,200,0x30};std::memcpy(p->data,&high,sizeof(high));
  FastCRC32 crc;p->crc32=crc.crc32(data+28,p->length-28);save("sdk2-high.bin",data,p->length);
  p->length=60;p->data_type=0;LivoxLidarImuRawPoint imu={1.25f,-2.5f,3.75f,0.5f,-0.25f,1.0f};std::memcpy(p->data,&imu,sizeof(imu));p->crc32=crc.crc32(data+28,p->length-28);save("sdk2-imu.bin",data,p->length);
}
