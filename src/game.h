// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_GAME_H
#define SSC_GAME_H

#include "global.h"
#include "model.h"
#include "control.h"
#include "level.h"
#include "menu.h"
#include "handler.h"

class Game
{
private:
	friend class Lunatic;
	Game();
	static Game *mInstance;
public:
	enum GameMode
	{
		PLAY,
		MENU,
	};

	static Game *getInstance()
	{
		if (!mInstance) mInstance = new Game;
		return mInstance;
	}

	void loop();
	void delay();

	void setMode(GameMode);
	inline GameMode getMode() { return mMode; }
	inline void drawMenu() { mGameMenu.draw(); }

private:
	Model *mModel;

	Controller mController;
	PlayHandler mPlayHandler;
	MenuHandler mMenuHandler;

	GameMenu mGameMenu;
	Level mLevel;

	double dt;
	double mTime1, mTime2;
	int mWait;
	GameMode mMode;
};

#endif // SSC_GAME_H
