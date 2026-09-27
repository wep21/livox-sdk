// Public-API integration client linked to unmodified Livox-SDK2.
#include "livox_lidar_api.h"
#include "FastCRC/FastCRC.h"
#include <atomic>
#include <chrono>
#include <cstdio>
#include <thread>
static std::atomic<unsigned> connected{0}, started{0}, points{0}, invalid{0};
static void mode(livox_status status,uint32_t,LivoxLidarAsyncControlResponse* r,void*) {
  if(status==kLivoxLidarStatusSuccess&&r&&r->ret_code==0)started++;
}
static void info(uint32_t handle,const LivoxLidarInfo* p,void*) {
  if(!p)return;
  std::printf("CONNECTED type=%u sn=%s ip=%s\n",p->dev_type,p->sn,p->lidar_ip);std::fflush(stdout);
  connected++;SetLivoxLidarWorkMode(handle,kLivoxLidarNormal,mode,nullptr);
}
static void data(uint32_t,uint8_t type,LivoxLidarEthernetPacket* p,void*) {
  if(!p||type!=9||p->version!=0||p->data_type!=1||p->dot_num!=96||p->length!=1380){invalid++;return;}
  FastCRC32 crc;auto* raw=reinterpret_cast<uint8_t*>(p);
  if(crc.crc32(raw+28,p->length-28)!=p->crc32){invalid++;return;}
  points++;
}
int main(int argc,char** argv) {
  if(argc!=2)return 2;
  DisableLivoxSdkConsoleLogger();
  if(!LivoxLidarSdkInit(argv[1],nullptr)){std::puts("INIT_FAILED");return 3;}
  SetLivoxLidarInfoChangeCallback(info,nullptr);SetLivoxLidarPointCloudCallBack(data,nullptr);
  if(!LivoxLidarSdkStart()){LivoxLidarSdkUninit();return 4;}
  for(int i=0;i<200&&points.load()<10;i++)std::this_thread::sleep_for(std::chrono::milliseconds(100));
  LivoxLidarSdkUninit();
  std::printf("RESULT connected=%u started=%u points=%u invalid=%u\n",connected.load(),started.load(),points.load(),invalid.load());
  return connected&&started&&points>=10&&!invalid?0:1;
}
