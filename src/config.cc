#include "config.h"

#include <cstring>
#include <cstdio>

extern "C"
{
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
}

const unsigned int MAX_LEN = 256;

bool isfile(const char *path)
{
	struct stat st;

	if (0 == stat(path, &st))
		return true;
	return false;
}

char HOMEDIR_CONF[MAX_LEN];

const char *PATH1 = "/.ssc/ssc.conf";
const char *PATH2 = "/usr/local/share/ssc/ssc.conf";
const char *PATH3 = "/usr/share/ssc/ssc.conf";
const char *PATH4 = "ssc.conf";
const char *PATH5 = "../ssc.conf";

const char *configFile()
{
	strncpy(HOMEDIR_CONF, getenv("HOME"), (MAX_LEN-1));
	strncat(HOMEDIR_CONF, PATH1, strlen(HOMEDIR_CONF)-(MAX_LEN-1));

	if (isfile(HOMEDIR_CONF)) return HOMEDIR_CONF;
	if (isfile(PATH2)) return PATH2;
	if (isfile(PATH3)) return PATH3;
	if (isfile(PATH4)) return PATH4;
	if (isfile(PATH5)) return PATH5;
	return 0;
}



class ConfigParser
{
public:
	enum ParseState
	{
		TOPLEVEL,
		OPEN_BRACK,
		SECTION,
		CLOSE_BRACK,
		IDENT,
		EQUAL,
		VALUE,
	};

	ConfigParser(Config *config)
		: mState(TOPLEVEL),
		  mIndex(0),
		  mPath(configFile()),
		  mConfig(config)
	{
		resetBuffer();
		if (mPath) parse();
	}

	void parse();

	bool streq(const char *a, const char *b)
	{
		return (!strcasecmp(a, b));
	}

	unsigned int getUnsigned()
	{
		return (unsigned int) strtoul(mValue, (char **) NULL, 10);
	}

	double getDouble()
	{
		return strtod(mValue, (char **) NULL);
	}

	bool getBool()
	{
		if (streq(mValue, "true"))
			return true;
		return false;
	}


	bool identIs(const char *s) { return streq(mIdent, s); }
	bool sectionIs(const char *s) { return streq(mSection, s); }

	void error()
	{
		fprintf(stderr, "error\n");
	}

	void applyVideo()
	{
		if (identIs("width"))
			mConfig->mResolution.x = getUnsigned();
		else if (identIs("height"))
			mConfig->mResolution.y = getUnsigned();
		else if (identIs("depth"))
			mConfig->mColorDepth = getUnsigned();
		else if (identIs("fullscreen"))
			mConfig->mFullScreen = getBool();
		else
			error();
	}

	void applyGame()
	{
		if (identIs("width"))
			mConfig->mGameArea.x = getUnsigned();
		else if (identIs("height"))
			mConfig->mGameArea.x = getUnsigned();
		else
			error();
	}

	void applyCamera()
	{
		if (identIs("fieldOfView"))
			mConfig->mFOV = getDouble();
		else if (identIs("zNear"))
			mConfig->mZNear = getDouble();
		else if (identIs("zFar"))
			mConfig->mZFar = getDouble();
		else
			error();
	}

	void applyAudio()
	{
		if (identIs("play_sound"))
			mConfig->mPlaySound = getBool();
		else if (identIs("play_music"))
			mConfig->mPlayMusic = getBool();
		else if (identIs("sound_volume"))
			mConfig->mSoundVol = getDouble();
		else if (identIs("music_volume"))
			mConfig->mMusicVol = getDouble();
		else
			error();
	}


	void applyValue()
	{
		if      (sectionIs("video"))	applyVideo();
		else if (sectionIs("game"))	applyGame();
		else if (sectionIs("camera"))	applyCamera();
		else if (sectionIs("audio"))	applyAudio();
		else
			error();
	}

	void resetBuffer()
	{
		mIndex = 0;
		mBuf[0] = '\0';
		mBuf[1] = '\0';
	}



	void handleChar(unsigned char c)
	{
		assert(mIndex < (MAX_LEN-1));
		mBuf[mIndex] = c;
		mBuf[mIndex+1] = '\0';
		mIndex++;
	}

private:
	ParseState mState;
	unsigned int mIndex;
	char mBuf[MAX_LEN];
	const char *mPath;
	Config *mConfig;

	char mSection[MAX_LEN];
	char mIdent[MAX_LEN];
	char mValue[MAX_LEN];
};


void ConfigParser::parse()
{
	FILE *fp = fopen(mPath, "r");

	unsigned char c = '\0';
	while (!feof(fp))
	{
		switch (c)
		{
		case '\0':
			break;
		case '[':
			mState = OPEN_BRACK;
			break;
		case ']':
			strncpy(mSection, mBuf, MAX_LEN);
			resetBuffer();
			mState = CLOSE_BRACK;
			break;
		case '=':
			strncpy(mIdent, mBuf, MAX_LEN);
			resetBuffer();
			mState = EQUAL;
			break;
		case '\n':
			if (mState == VALUE)
			{
				strncpy(mValue, mBuf, MAX_LEN);
				applyValue();
			}
			resetBuffer();
			mState = TOPLEVEL;
			break;
		case ' ':
		case '\t':
		case '\r':
			break;
		default:
			switch (mState)
			{
			case IDENT:
			case VALUE:
			case SECTION:
				break;
			case TOPLEVEL:
				mState = IDENT;
				break;
			case OPEN_BRACK:
				mState = SECTION;
				break;
			case CLOSE_BRACK:
				mState = IDENT;
				break;
			case EQUAL:
				mState = VALUE;
				break;
			}
			handleChar(c);
			break;
		}
		c = (unsigned char) fgetc(fp);
	}
	fclose(fp);
}

// -------------------------------------------------------

Config* Config::mInstance = 0;

Config::Config()
	: mResolution(800, 600),
	  mGameArea(4000, 4000),
	  mFOV(45),
	  mZNear(1),
	  mZFar(15000),
	  mPlaySound(false),
	  mPlayMusic(false),
	  mSoundVol(.2),
	  mMusicVol(1),
	  mColorDepth(24),
	  mFullScreen(false)
{
	ConfigParser(this);
}

void Config::handleArguments(int argc, char **argv)
{
}
