#ifndef SSC_CONFIG_H
#define SSC_CONFIG_H

#include "common.h"
#include "coord.h"

class Config
{
        // ------------------------------------------------------------------
        //
        // Lifecycle managment
        //
        // ------------------------------------------------------------------

private:
        friend class ConfigParser;
        Config();
        static Config* mInstance;
public:
        static void destroy();

        static Config* getInstance()
        {
                if (!mInstance)
                        mInstance = new Config;
                return mInstance;
        }

        // ------------------------------------------------------------------
        //
        // Major operations
        //
        // ------------------------------------------------------------------

        void readFile();
        void saveFile();
        void handleArguments(int argc, char **argv);

        // ------------------------------------------------------------------
        //
        // Accessors
        //
        // ------------------------------------------------------------------

        // video and game
        Coord2<int> getResolution() { return mResolution; }
        Coord2<int> getGameArea() { return mGameArea; }
        unsigned int getColorDepth() { return mColorDepth; }
        bool fullscreen() { return mFullScreen; }

        // camera
        double getFOV() { return mFOV; }
        double getZNear() { return mZNear; }
        double getZFar() { return mZFar; }

        // audio
        bool playSound() { return mPlaySound; }
        bool playMusic() { return mPlayMusic; }
        double soundVol() { return mSoundVol; }
        double musicVol() { return mMusicVol; }

private:
        // ------------------------------------------------------------------
        //
        // Member variables
        //
        // ------------------------------------------------------------------

        Coord2<int> mResolution, mGameArea;
        double mFOV, mZNear, mZFar;
        bool mPlaySound, mPlayMusic;
        double mSoundVol, mMusicVol;
        unsigned int mColorDepth;
        bool mFullScreen;
};

#endif // SSC_CONFIG_H
